use crate::core::hprof::model::FieldType;
use std::io::{self, BufReader, Read};

pub struct ClassDump {
    pub class_id: u64,
    pub super_id: u64,
    pub loader_id: u64,
    pub instance_size: u64,
    pub static_fields: Vec<(u64, FieldType, Option<u64>)>,
    pub instance_fields: Vec<(u64, FieldType)>,
}

pub struct HprofReader<R: Read> {
    inner: BufReader<R>,
    id_size: usize,
    remaining: u64,
}

impl<R: Read> HprofReader<R> {
    pub fn new(inner: BufReader<R>, id_size: usize) -> Self {
        Self {
            inner,
            id_size,
            remaining: 0,
        }
    }

    pub fn id_size(&self) -> usize {
        self.id_size
    }

    pub fn remaining(&self) -> u64 {
        self.remaining
    }

    pub fn next_record(&mut self) -> io::Result<Option<u8>> {
        let mut tag = [0u8; 1];
        match self.inner.read_exact(&mut tag) {
            Ok(()) => {}
            Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(err) => return Err(err),
        }
        let mut buffer = [0u8; 4];
        self.inner.read_exact(&mut buffer)?; // time delta
        self.inner.read_exact(&mut buffer)?; // record length
        self.remaining = u32::from_be_bytes(buffer) as u64;
        Ok(Some(tag[0]))
    }

    fn read_bounded(&mut self, buffer: &mut [u8]) -> io::Result<()> {
        if self.remaining < buffer.len() as u64 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "record body is shorter than expected",
            ));
        }
        self.inner.read_exact(buffer)?;
        self.remaining -= buffer.len() as u64;
        Ok(())
    }

    pub fn read_u8(&mut self) -> io::Result<u8> {
        let mut buffer = [0u8; 1];
        self.read_bounded(&mut buffer)?;
        Ok(buffer[0])
    }

    pub fn read_u16(&mut self) -> io::Result<u16> {
        let mut buffer = [0u8; 2];
        self.read_bounded(&mut buffer)?;
        Ok(u16::from_be_bytes(buffer))
    }

    pub fn read_u32(&mut self) -> io::Result<u32> {
        let mut buffer = [0u8; 4];
        self.read_bounded(&mut buffer)?;
        Ok(u32::from_be_bytes(buffer))
    }

    pub fn read_id(&mut self) -> io::Result<u64> {
        if self.id_size == 8 {
            let mut buffer = [0u8; 8];
            self.read_bounded(&mut buffer)?;
            Ok(u64::from_be_bytes(buffer))
        } else {
            Ok(self.read_u32()? as u64)
        }
    }

    pub fn skip(&mut self, count: u64) -> io::Result<()> {
        if self.remaining < count {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "record body is shorter than the requested skip",
            ));
        }
        let mut left = count;
        let mut buffer = [0u8; 8192];
        while left > 0 {
            let take = left.min(buffer.len() as u64) as usize;
            self.inner.read_exact(&mut buffer[..take])?;
            left -= take as u64;
        }
        self.remaining -= count;
        Ok(())
    }

    pub fn skip_remaining(&mut self) -> io::Result<()> {
        let count = self.remaining;
        self.skip(count)
    }

    pub fn read_string_body(&mut self) -> io::Result<String> {
        let count = self.remaining;
        if count > 16 * 1024 * 1024 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "HPROF string record is unreasonably large",
            ));
        }
        let mut bytes = vec![0u8; count as usize];
        self.read_bounded(&mut bytes)?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }
}

pub fn read_header<R: Read>(reader: &mut BufReader<R>) -> io::Result<(String, u8, i64)> {
    let mut version_bytes = Vec::new();
    loop {
        let mut byte = [0u8; 1];
        reader.read_exact(&mut byte)?;
        if byte[0] == 0 {
            break;
        }
        version_bytes.push(byte[0]);
        if version_bytes.len() > 64 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "HPROF version string is too long",
            ));
        }
    }
    let version = String::from_utf8_lossy(&version_bytes).into_owned();
    if !version.starts_with("JAVA PROFILE 1.0") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("not an HPROF dump (header was {version:?})"),
        ));
    }
    let mut buffer = [0u8; 4];
    reader.read_exact(&mut buffer)?;
    let id_size = u32::from_be_bytes(buffer) as u8;
    if id_size != 4 && id_size != 8 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unsupported HPROF identifier size {id_size}"),
        ));
    }
    let mut timestamp = [0u8; 8];
    reader.read_exact(&mut timestamp)?;
    Ok((version, id_size, u64::from_be_bytes(timestamp) as i64))
}

pub fn read_class_dump<R: Read>(reader: &mut HprofReader<R>) -> io::Result<ClassDump> {
    let class_id = reader.read_id()?;
    let _stack_trace = reader.read_u32()?;
    let super_id = reader.read_id()?;
    let loader_id = reader.read_id()?;
    let _signers = reader.read_id()?;
    let _protection_domain = reader.read_id()?;
    let _reserved1 = reader.read_id()?;
    let _reserved2 = reader.read_id()?;
    let instance_size = reader.read_u32()? as u64;

    let constant_pool_count = reader.read_u16()?;
    for _ in 0..constant_pool_count {
        let _index = reader.read_u16()?;
        let ty = read_field_type(reader)?;
        if ty == FieldType::Object {
            let _ = reader.read_id()?;
        } else {
            reader.skip(ty.size(reader.id_size) as u64)?;
        }
    }

    let static_count = reader.read_u16()?;
    let mut static_fields = Vec::with_capacity(static_count as usize);
    for _ in 0..static_count {
        let name_id = reader.read_id()?;
        let ty = read_field_type(reader)?;
        let value = if ty == FieldType::Object {
            Some(reader.read_id()?)
        } else {
            reader.skip(ty.size(reader.id_size) as u64)?;
            None
        };
        static_fields.push((name_id, ty, value));
    }

    let instance_count = reader.read_u16()?;
    let mut instance_fields = Vec::with_capacity(instance_count as usize);
    for _ in 0..instance_count {
        let name_id = reader.read_id()?;
        let ty = read_field_type(reader)?;
        instance_fields.push((name_id, ty));
    }

    Ok(ClassDump {
        class_id,
        super_id,
        loader_id,
        instance_size,
        static_fields,
        instance_fields,
    })
}

fn read_field_type<R: Read>(reader: &mut HprofReader<R>) -> io::Result<FieldType> {
    let raw = reader.read_u8()?;
    FieldType::from_u8(raw).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unknown HPROF field type {raw}"),
        )
    })
}

pub fn read_root_body<R: Read>(
    reader: &mut HprofReader<R>,
    tag: u8,
) -> io::Result<(u64, &'static str)> {
    match tag {
        0xFF => Ok((reader.read_id()?, "Unknown")),
        0x01 => {
            let id = reader.read_id()?;
            let _jni_ref = reader.read_id()?;
            Ok((id, "JNI global"))
        }
        0x02 => {
            let id = reader.read_id()?;
            let _thread = reader.read_u32()?;
            let _depth = reader.read_u32()?;
            Ok((id, "JNI local"))
        }
        0x03 => {
            let id = reader.read_id()?;
            let _thread = reader.read_u32()?;
            let _depth = reader.read_u32()?;
            Ok((id, "Java frame"))
        }
        0x04 => {
            let id = reader.read_id()?;
            let _thread = reader.read_u32()?;
            Ok((id, "Native stack"))
        }
        0x05 => Ok((reader.read_id()?, "Sticky class")),
        0x06 => {
            let id = reader.read_id()?;
            let _thread = reader.read_u32()?;
            Ok((id, "Thread block"))
        }
        0x07 => Ok((reader.read_id()?, "Monitor used")),
        0x08 => {
            let id = reader.read_id()?;
            let _thread = reader.read_u32()?;
            let _stack = reader.read_u32()?;
            Ok((id, "Thread object"))
        }
        // Android/other HPROF producers emit additional root kinds. They all
        // carry a single object id, so they are safe to treat as roots.
        0x89 => Ok((reader.read_id()?, "Interned string")),
        0x8A => Ok((reader.read_id()?, "Finalizing")),
        0x8B => Ok((reader.read_id()?, "Debugger")),
        0x8C => Ok((reader.read_id()?, "Reference cleanup")),
        0x8D => Ok((reader.read_id()?, "VM internal")),
        0x90 => Ok((reader.read_id()?, "Unreachable")),
        other => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unknown HPROF heap subrecord tag 0x{other:02x}"),
        )),
    }
}
