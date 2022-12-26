[![pipeline](https://github.com/d-e-s-o/coredump/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/d-e-s-o/coredump/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/coredump.svg)](https://crates.io/crates/coredump)
[![Docs](https://docs.rs/coredump/badge.svg)](https://docs.rs/coredump)
[![rustc](https://img.shields.io/badge/rustc-1.34+-blue.svg)](https://blog.rust-lang.org/2019/04/11/Rust-1.34.0.html)

coredump
========

- [Documentation][docs-rs]
- [Changelog](CHANGELOG.md)

**coredump** is a crate to make a Rust program create a core dump when
it encounters a panic.


Why?
----

By default, when a Rust program panics it will print a backtrace and
terminate the program. There are a few situations when this behavior is
not sufficient or desired, including when:

- you want to make sure that you are able to root cause problems based
  on bug reports and a backtrace alone may not be enough
- you cannot get the backtrace of a panic, for example, because it is
  printed to the terminal's alternate screen or otherwise not available

**coredump** caters to those and more cases. It should combine with and
may be a nice addition to other crates involved in the panic path, such
as [`human-panic`][human-panic].


Usage
-----

The crate works by registering a custom panic handler that will run
after the previously installed (or default) one. Registration of this
custom handler is as simple as invoking:
```rust
register_panic_handler()
```
early during program initialization (ideally before any panic may
happen).

After a panic has happened, the core file can be investigated using
[`gdb(1)`][man-1-gdb], like so:
```bash
$ rust-gdb <paniced-binary> <core-file>
```

By default a core file as created by this crate will reside in the
system's temp directory, but this behavior may be overwritten by system
configuration.


Limitations
-----------

By design, this crate is concerned only with regular (language level)
panics. Segmentation violations or other problems are not covered by its
approach. The application using this crate also will have to be compiled
with proper unwinding support and not just abort execution (unwinding is
the case by default but could be overwritten by adding `panic = 'abort'`
to the profile being compiled against).

Also note that while core dumping support is present on many systems,
many factors play into the ability of a system to create an application
core dump. A lot of these factors are out of this crate's control, both
in the sense that they cannot be changed but also that they may
potentially not even be checkable at runtime. The full list of
requirements the system must meet is detailed in
[`core(5)`][man-5-core].


[docs-rs]: https://docs.rs/crate/coredump
[human-panic]: https://crates.io/crates/human-panic
[man-1-gdb]: http://man7.org/linux/man-pages/man1/gdb.1.html
[man-5-core]: http://man7.org/linux/man-pages/man5/core.5.html
