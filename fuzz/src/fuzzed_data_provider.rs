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
