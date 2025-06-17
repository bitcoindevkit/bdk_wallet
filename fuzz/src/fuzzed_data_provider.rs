#[macro_export]
macro_rules! try_consume_byte {
    ($data_iter:expr) => {
        match $data_iter.next() {
            Some(byte) => byte,
            None => return,
        }
    };
}

#[macro_export]
macro_rules! try_consume_bool {
    ($data_iter:expr) => {
        match $data_iter.next() {
            Some(byte) => *byte != 0,
            None => return,
        }
    };
}

#[macro_export]
macro_rules! try_consume_u8 {
    ($data_iter:expr) => {
        match $data_iter.next() {
            Some(byte) => *byte,
            None => return,
        }
    };
}

#[macro_export]
macro_rules! try_consume_u32 {
    ($data_iter:expr) => {{
        let mut bytes = [0u8; 4];
        for i in 0..4 {
            match $data_iter.next() {
                Some(byte) => bytes[i] = *byte,
                None => return,
            }
        }
        u32::from_le_bytes(bytes)
    }};
}

#[macro_export]
macro_rules! try_consume_u64 {
    ($data_iter:expr) => {{
        let mut bytes = [0u8; 8];
        for i in 0..8 {
            match $data_iter.next() {
                Some(byte) => bytes[i] = *byte,
                None => return,
            }
        }
        u64::from_le_bytes(bytes)
    }};
}

pub fn consume_bytes(data: &mut &[u8], num_bytes: usize) -> Vec<u8> {
    let num_bytes = num_bytes.min(data.len());

    let (bytes, remaining) = data.split_at(num_bytes);
    *data = remaining;

    bytes.to_vec()
}

pub fn consume_u64(data: &mut &[u8]) -> u64 {
    // We need at least 8 bytes to read a u64
    if data.len() < 8 {
        return 0;
    }

    let (u64_bytes, rest) = data.split_at(8);
    *data = rest;

    u64::from_le_bytes([
        u64_bytes[0],
        u64_bytes[1],
        u64_bytes[2],
        u64_bytes[3],
        u64_bytes[4],
        u64_bytes[5],
        u64_bytes[6],
        u64_bytes[7],
    ])
}

pub fn consume_u32(data: &mut &[u8]) -> u32 {
    // We need at least 4 bytes to read a u32
    if data.len() < 4 {
        return 0;
    }

    let (u32_bytes, rest) = data.split_at(4);
    *data = rest;

    u32::from_le_bytes([u32_bytes[0], u32_bytes[1], u32_bytes[2], u32_bytes[3]])
}

pub fn consume_u8(data: &mut &[u8]) -> u8 {
    // We need at least 1 byte to read a u8
    if data.is_empty() {
        return 0;
    }

    let (u8_bytes, rest) = data.split_at(1);
    *data = rest;

    u8::from_le_bytes([u8_bytes[0]])
}

pub fn consume_bool(data: &mut &[u8]) -> bool {
    (1 & consume_u8(data)) != 0
}

pub fn consume_byte(data: &mut &[u8]) -> u8 {
    let byte = data[0];
    *data = &data[1..];

    byte
}
