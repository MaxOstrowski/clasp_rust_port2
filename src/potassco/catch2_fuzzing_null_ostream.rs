use std::io::{self, Write};

pub struct NullStreambuf {
    dummy_buffer: [u8; 64],
    put_len: usize,
}

impl Default for NullStreambuf {
    fn default() -> Self {
        Self {
            dummy_buffer: [0; 64],
            put_len: 64,
        }
    }
}

impl NullStreambuf {
    pub fn overflow(&mut self, byte: Option<u8>) -> u8 {
        self.put_len = self.dummy_buffer.len();
        byte.unwrap_or(b'\0')
    }

    pub fn available_put_area(&self) -> usize {
        self.put_len
    }
}

pub struct NullOStream {
    streambuf: NullStreambuf,
}

impl NullOStream {
    pub fn new() -> Self {
        Self {
            streambuf: NullStreambuf::default(),
        }
    }

    pub fn rdbuf(&mut self) -> &mut NullStreambuf {
        &mut self.streambuf
    }

    pub fn avoid_out_of_line_virtual_compiler_warning(&mut self) {}
}

impl Default for NullOStream {
    fn default() -> Self {
        Self::new()
    }
}

impl Write for NullOStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let _ = buf;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
