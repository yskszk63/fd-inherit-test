use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use std::os::unix::io::AsRawFd;
use std::io;
use std::os::unix::io::RawFd;

use tokio::process::Command;

fn dup(fd: RawFd) -> io::Result<RawFd> {
    let r = unsafe {
        libc::dup(fd)
    };
    if r == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(r)
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

fn close(fd: RawFd) -> io::Result<()> {
    let r = unsafe {
        libc::close(fd)
    };
    if r == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn set_blocking(fd: RawFd) -> io::Result<()> {
    let r = unsafe {
        libc::fcntl(fd, libc::F_GETFL)
    };
    if r == -1 {
        return Err(io::Error::last_os_error());
    }

    if r & libc::O_NONBLOCK != 0 {
        let r = unsafe {
            libc::fcntl(fd, libc::F_SETFL, r ^ libc::O_NONBLOCK)
        };
        if r == -1 {
            return Err(io::Error::last_os_error());
        }
    }

    Ok(())
}

#[tokio::test]
async fn test_asynchronous() -> io::Result<()> {
    let script = r#"#!/usr/bin/env python3
import os, shutil
print(os.stat(3))
with os.fdopen(3, 'rb') as r:
    with os.fdopen(4, 'wb') as w:
        shutil.copyfileobj(r, w)
"#;

    let (input, mut their_input) = tokio_pipe::pipe()?;
    let (mut their_output, output) = tokio_pipe::pipe()?;

    let mut command = Command::new("python3");
    command.args(["-c", script]);

    set_blocking(input.as_raw_fd())?;
    set_blocking(output.as_raw_fd())?;

    let mut child = unsafe {
        let input = input.as_raw_fd();
        let output = output.as_raw_fd();
        command.pre_exec(move || {
            let no_cloexec_input = dup(input)?;
            let no_cloexec_output = dup(output)?;

            dup2(no_cloexec_input, 3)?;
            dup2(no_cloexec_output, 4)?;

            close(no_cloexec_input)?;
            close(no_cloexec_output)?;

            Ok(())
        }).spawn()?
    };
    drop(input);
    drop(output);

    their_input.write_all(&b"Hello, World!"[..]).await?;
    drop(their_input);

    let mut buf = vec![];
    their_output.read_to_end(&mut buf).await?;

    assert_eq!(&b"Hello, World!"[..], &buf);

    assert!(child.wait().await?.success());
    Ok(())
}
