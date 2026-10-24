use anyhow::{Context, Result, ensure};
use std::io::{Read, Write};

pub(crate) const MAX_MESSAGE: usize = 64 * 1024;

pub(crate) fn read_message(reader: &mut impl Read) -> Result<Option<Vec<u8>>> {
    let mut length = [0; 4];
    match reader.read_exact(&mut length[..1]) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error.into()),
    }
    reader
        .read_exact(&mut length[1..])
        .context("Truncated message length")?;
    let length = u32::from_ne_bytes(length) as usize;
    ensure!(
        length > 0 && length <= MAX_MESSAGE,
        "Invalid native message length"
    );
    let mut message = vec![0; length];
    reader
        .read_exact(&mut message)
        .context("Truncated message body")?;
    Ok(Some(message))
}

pub(crate) fn write_message(writer: &mut impl Write, message: &[u8]) -> Result<()> {
    ensure!(message.len() <= MAX_MESSAGE, "Native response is too large");
    writer.write_all(&(message.len() as u32).to_ne_bytes())?;
    writer.write_all(message)?;
    writer.flush()?;
    Ok(())
}
