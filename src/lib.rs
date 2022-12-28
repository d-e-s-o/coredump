// lib.rs

// Copyright (C) 2019-2022 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

//! A module for making the program dump core on panics (on a best
//! effort basis).

use std::borrow::Cow;
use std::convert::TryInto;
use std::env::current_dir;
use std::env::set_current_dir;
use std::env::temp_dir;
use std::error::Error as StdError;
use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt::Result as FmtResult;
use std::io::Error as IoError;
use std::io::ErrorKind;
use std::num::TryFromIntError;
use std::panic::set_hook;
use std::panic::take_hook;
use std::path::Path;
use std::process::id as pid;

use libc::getrlimit;
use libc::kill;
use libc::rlimit;
use libc::setrlimit;
use libc::RLIMIT_CORE;
use libc::SIGQUIT;


type Str = Cow<'static, str>;


/// The error type used by this crate.
#[derive(Debug)]
pub enum Error {
  Io(IoError),
  Int(TryFromIntError),
}

impl StdError for Error {
  fn source(&self) -> Option<&(dyn StdError + 'static)> {
    match self {
      Error::Io(err) => err.source(),
      Error::Int(err) => err.source(),
    }
  }
}

impl Display for Error {
  fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
    match self {
      Error::Io(err) => write!(f, "{}", err),
      Error::Int(err) => write!(f, "{}", err),
    }
  }
}

impl From<IoError> for Error {
  fn from(e: IoError) -> Self {
    Error::Io(e)
  }
}

impl From<TryFromIntError> for Error {
  fn from(e: TryFromIntError) -> Self {
    Error::Int(e)
  }
}


/// A helper trait for annotating errors with some context.
trait WithCtx<T>
where
  Self: Sized,
{
  type E;

  fn ctx<F, S>(self, ctx: F) -> Result<T, (Str, Self::E)>
  where
    F: Fn() -> S,
    S: Into<Str>;
}

impl<T, E> WithCtx<T> for Result<T, E> {
  type E = E;

  fn ctx<F, S>(self, ctx: F) -> Result<T, (Str, Self::E)>
  where
    F: Fn() -> S,
    S: Into<Str>,
  {
    self.map_err(|e| (ctx().into(), e))
  }
}


/// Check the return value of a call (typically into libc) that modifies
/// the last error variable in case of error.
fn check<T>(result: T, error: T) -> Result<(), Error>
where
  T: Copy + PartialOrd<T>,
{
  if result == error {
    Err(IoError::last_os_error())?;
  }
  Ok(())
}


/// Force a core dump of the process by sending SIGQUIT to it.
fn dump_core() -> Result<(), (Str, Error)> {
  let pid = pid();
  let pid = pid.try_into().map_err(Error::from).ctx(|| {
    format!(
      "unable to dump core: PID {} is not a valid unsigned value",
      pid
    )
  })?;

  check(unsafe { kill(pid, SIGQUIT) }, -1).ctx(|| "failed to send SIGQUIT")?;
  Ok(())
}


/// Create a core dump of the process in the given directory by killing
/// it.
fn dump_core_and_quit(dir: &Path) -> Result<(), (Str, Error)> {
  // We try to change the working directory to the system's temp dir to
  // have the core dump generated there. Note that this is a best-effort
  // action. It is even possible that that core file pattern (on a Linux
  // system) contains an absolute path in which case the working
  // directory likely doesn't matter at all.
  let cur_dir = current_dir()
    .map_err(Error::from)
    .ctx(|| "failed to retrieve current directory")?;

  set_current_dir(dir)
    .map_err(Error::from)
    .ctx(|| format!("failed to change working directory to {}", dir.display()))?;

  if let Err(err) = dump_core() {
    // Opportunistically restore the working directory. We probably
    // won't continue to run because the panic will propagate up, but
    // let's plan for all cases.
    // We can't do much about an error at this point. So just ignore
    // it...
    let _ = set_current_dir(cur_dir);
    Err(err)
  } else {
    Ok(())
  }
}


/// Enable core dumps to file by ensuring that the respective rlimit is
/// set correctly.
/// Note that we do not touch the name under which a core file is
/// created. At least on Linux that is a global property and we do not
/// want to change it for that reason.
fn enable_core_dumps() -> Result<(), (Str, Error)> {
  let mut limit = rlimit {
    rlim_cur: 0,
    rlim_max: 0,
  };

  check(unsafe { getrlimit(RLIMIT_CORE, &mut limit) }, -1)
    .ctx(|| "failed to retrieve core file size limit")?;

  // There is no way for us to know what a sufficiently large core file
  // size would be, but we know for sure that 0 ain't it (as it
  // effectively means we can't create a core file at all).
  if limit.rlim_max == 0 {
    Err(IoError::new(ErrorKind::Other, "hard limit is zero").into())
      .ctx(|| "failed to adjust core file size limit")?;
  }

  // As an application we are only allowed to touch the soft limit
  // (`rlim_cur`), while the hard limit acts as a ceiling. We bump it
  // as high as we can.
  limit.rlim_cur = limit.rlim_max;

  // TODO: There is also setrlimit64. Find out what its deal is and
  //       whether we want/need it.
  check(unsafe { setrlimit(RLIMIT_CORE, &limit) }, -1)
    .ctx(|| "failed to adjust core file size limit")?;

  Ok(())
}


/// Register a panic handler that will cause the program to dump core.
///
/// Note that creating a coredump is best effort, as the process largely
/// depends on system configuration. For example, on a Linux system the
/// kernel needs to have coredump support and coredump must not have
/// been prohibited (e.g., caused by a zero core file size rlimit).
/// Furthermore, the name of the resulting core file may be generic and
/// not reflect the program that crashed. On Linux it can be inquired
/// via `/proc/sys/kernel/core_pattern`.
pub fn register_panic_handler() -> Result<(), (Str, Error)> {
  enable_core_dumps()?;

  // The default panic handler is nice in that it allows for retrieving
  // the backtrace at the time of the panic on the user's discretion. We
  // want to preserve this functionality and cannot easily reimplement
  // it without pulling in additional dependencies. Hence, we
  // effectively just wrap it by adding a step afterwards.
  let default_panic = take_hook();

  set_hook(Box::new(move |panic_info| {
    default_panic(panic_info);

    // We have no real way to bubble up the error, so we can only print
    // it. Strictly speaking we should use the same output that the
    // default panic handler would use, but we can't access the
    // underlying object. So just print it to stderr.
    if let Err((ctx, err)) = dump_core_and_quit(&temp_dir()) {
      eprintln!("failed to dump core: {}: {}", ctx, err);
    }
  }));

  Ok(())
}
