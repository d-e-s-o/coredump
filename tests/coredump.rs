// coredump.rs

// Copyright (C) 2019 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

// Note that this file should only contain a single test. The test in
// here invokes itself and having multiple test running in parallel
// while that is happening is probably a bad idea.

use std::env::current_exe;
use std::env::temp_dir;
use std::env::var_os;
use std::fs::read_to_string;
use std::fs::remove_file;
use std::io::ErrorKind;
use std::os::unix::process::ExitStatusExt;
use std::process::Command;

use libc::SIGQUIT;

use coredump::register_panic_handler;

const CHILD_MARKER: &str = "PANICING_CHILD";


#[cfg(not(target_os = "linux"))]
compile_error!("only Linux is supported currently");


#[test]
#[cfg(target_os = "linux")]
fn dump_core() {
  if var_os(CHILD_MARKER).is_none() {
    // Note that the core pattern presumably could be many things and
    // half of them would make our test not succeed. We just use it to
    // do something slightly more clever than hardcoding "core", but
    // ultimately are willing to only put so much effort into this
    // test...
    let tmp_dir = temp_dir();
    let core_pattern = read_to_string("/proc/sys/kernel/core_pattern").unwrap();
    // A core file pattern beginning with a pipe symbol means that the
    // core dump is actually redirected to a program. That's a system
    // configuration we probably cannot and most likely do not want to
    // change, so if the system is configured this way (systemd enabled
    // systems likely are), this test cannot run.
    if core_pattern.starts_with('|') {
      return
    }
    let core_file = tmp_dir.join(core_pattern.trim_end());

    match remove_file(&core_file) {
      Ok(()) => (),
      Err(ref err) if err.kind() == ErrorKind::NotFound => (),
      Err(err) => panic!("unexpected error: {}", err),
    };

    let rc = Command::new(current_exe().unwrap())
      .env_clear()
      .env(CHILD_MARKER, "true")
      .status()
      .unwrap();

    assert!(!rc.success());
    assert_eq!(rc.signal().unwrap(), SIGQUIT);
    assert!(
      core_file.exists(),
      "core file {} does not exist",
      core_file.display(),
    );
    let _ = remove_file(&core_file);
  } else {
    register_panic_handler().unwrap();
    panic!("induced panic");
  }
}
