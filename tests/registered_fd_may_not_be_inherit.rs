use std::io;
use std::os::raw::c_int;
use std::os::unix::io::RawFd;
use std::os::unix::prelude::CommandExt;
use std::process::Command;

use tokio::io::unix::AsyncFd;

fn set_nonblocking(fd: RawFd) -> io::Result<()> {
    let r = unsafe {
        libc::fcntl(fd, libc::F_GETFL)
    };
    if r == -1 {
        return Err(io::Error::last_os_error());
    }

    let r = unsafe {
        libc::fcntl(fd, libc::F_SETFL, r | libc::O_NONBLOCK)
    };
    if r == -1 {
        return Err(io::Error::last_os_error());
    }

    Ok(())
}

fn set_cloexec(fd: RawFd) -> io::Result<()> {
    let r = unsafe {
        libc::fcntl(fd, libc::F_GETFD)
    };
    if r == -1 {
        return Err(io::Error::last_os_error());
    }

    let r = unsafe {
        libc::fcntl(fd, libc::F_SETFD, r | libc::FD_CLOEXEC)
    };
    if r == -1 {
        return Err(io::Error::last_os_error());
    }

    Ok(())
}

fn set_nocloexec(fd: RawFd) -> io::Result<()> {
    let r = unsafe {
        libc::fcntl(fd, libc::F_GETFD)
    };
    if r == -1 {
        return Err(io::Error::last_os_error());
    }

    if r & libc::FD_CLOEXEC != 0 {
        let r = unsafe {
            libc::fcntl(fd, libc::F_SETFD, r ^ libc::FD_CLOEXEC)
        };
        if r == -1 {
            return Err(io::Error::last_os_error());
        }
    }

    Ok(())
}

fn new_cloexec_nonblocking_pipe() -> io::Result<(c_int, c_int)> {
    let mut pair = [0; 2];
    let r = unsafe {
        libc::pipe(pair.as_mut_ptr()) // no CLOEXEC (pipe2 does not exist on mac)
    };
    if r == -1 {
        return Err(io::Error::last_os_error());
    }

    for fd in pair {
        set_cloexec(fd)?;
        set_nonblocking(fd)?;
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

/*
fn dup(fd: RawFd) -> io::Result<RawFd> {
    let r = unsafe {
        libc::dup(fd)
    };
    if r == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(r)
}
*/

fn dup2(fd: RawFd, newfd: RawFd) -> io::Result<()> {
    let r = unsafe {
        libc::dup2(fd, newfd)
    };
    if r == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[tokio::test]
async fn test_registered_fd_mai_not_be_inherit() -> io::Result<()> {
    // CLOEXEC, NONBLOCKING?????????????????????
    let (r, w) = new_cloexec_nonblocking_pipe()?;

    // tokio???I/O Driver?????????
    // Linux???epoll, Mac???kqueue
    let r = AsyncFd::new(r)?;

    // 3?????????????????????????????????stat??????python???????????????
    let script = r#"#!/usr/bin/env python3
import os
print(os.stat(3))
"#;

    let mut command = Command::new("python3");
    command.args(["-c", script]);
    let mut child = unsafe {
        // AsyncFd????????????????????????????????????????????????????????????
        // AsyncFd????????????????????????
        let rfd = *r.get_ref();
        command.pre_exec(move || {
            // python???stat??????3??????????????????????????????????????????
            if rfd == 3 {
                set_nocloexec(rfd)?;
            } else {
                dup2(rfd, 3)?;
            }
            Ok(())
        });
        let result = command.spawn();
        // pre_exec????????????????????????AsynFd???drop
        drop(r);
        result
    }?;

    let ok = child.wait()?.success();
    assert!(ok);

    close(w)?;
    Ok(())
}
