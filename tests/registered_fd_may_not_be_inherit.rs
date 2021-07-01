use std::io;
use std::os::raw::c_int;
use std::os::unix::io::RawFd;
use std::os::unix::prelude::CommandExt;
use std::process::Command;

use mio::Interest;
use mio::Poll;
use mio::Token;
use mio::unix::SourceFd;

fn pipe() -> io::Result<(c_int, c_int)> {
    let mut pair = [0; 2];
    let r = unsafe {
        libc::pipe(pair.as_mut_ptr()) // no CLOEXEC (pipe2 does not exist on mac)
    };
    if r == -1 {
        return Err(io::Error::last_os_error());
    }
    let [r, w] = pair;

    Ok((r, w))
}

fn close(fd: RawFd) -> io::Result<()> {
    let r = unsafe {
        libc::close(fd)
    };
    if r == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn dup2(fd: RawFd, newfd: RawFd) -> io::Result<()> {
    let r = unsafe {
        libc::dup2(fd, newfd)
    };
    if r == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[test]
fn test_registered_fd_mai_not_be_inherit() -> io::Result<()> {
    let (r, w) = pipe()?;
    close(w)?;
    drop(w);

    let poll = Poll::new()?;
    poll.registry().register(&mut SourceFd(&r), Token(0), Interest::READABLE | Interest::WRITABLE)?;

    let script = r#"#!/usr/bin/env python3
import os
print(os.stat(3))
"#;
    let mut command = Command::new("python3");
    command.args(["-c", script]);
    let mut child = unsafe {
        command.pre_exec(move || {
            dup2(r, 3)?;
            Ok(())
        });
        let result = command.spawn();
        close(r)?;
        result
    }?;

    let ok = child.wait()?.success();
    assert!(ok);

    drop(poll);
    Ok(())
}
