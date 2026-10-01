use std::{
    io::Read,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

const STREAM_BUFFER_SIZE: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileHashError {
    ReadFailed,
    Cancelled,
}

impl std::fmt::Display for FileHashError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::ReadFailed => "读取待校验文件失败",
            Self::Cancelled => "文件校验已取消",
        })
    }
}

impl std::error::Error for FileHashError {}

/// 文件哈希任务的轻量取消令牌，可安全跨线程共享。
#[derive(Debug, Clone, Default)]
pub(crate) struct FileHashCancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl FileHashCancellationToken {
    pub(crate) fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

pub(crate) fn hash_reader<R: Read>(
    mut reader: R,
    cancellation: &FileHashCancellationToken,
) -> Result<String, FileHashError> {
    let mut buffer = [0_u8; STREAM_BUFFER_SIZE];
    let mut hash = Sha256::new();
    loop {
        if cancellation.is_cancelled() {
            return Err(FileHashError::Cancelled);
        }
        let read = reader
            .read(&mut buffer)
            .map_err(|_| FileHashError::ReadFailed)?;
        if read == 0 {
            break;
        }
        hash.update(&buffer[..read]);
    }
    if cancellation.is_cancelled() {
        return Err(FileHashError::Cancelled);
    }
    Ok(hash.finalize_hex())
}

// 小型标准库实现避免仅为流式哈希增加发布依赖。
struct Sha256 {
    state: [u32; 8],
    buffer: [u8; 64],
    buffered: usize,
    total_bytes: u64,
}

impl Sha256 {
    fn new() -> Self {
        Self {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            buffer: [0; 64],
            buffered: 0,
            total_bytes: 0,
        }
    }

    fn update(&mut self, mut input: &[u8]) {
        self.total_bytes = self.total_bytes.wrapping_add(input.len() as u64);
        if self.buffered > 0 {
            let count = (64 - self.buffered).min(input.len());
            self.buffer[self.buffered..self.buffered + count].copy_from_slice(&input[..count]);
            self.buffered += count;
            input = &input[count..];
            if self.buffered < 64 {
                return;
            }
            let block = self.buffer;
            self.compress(&block);
            self.buffered = 0;
        }
        while input.len() >= 64 {
            self.compress(&input[..64]);
            input = &input[64..];
        }
        self.buffer[..input.len()].copy_from_slice(input);
        self.buffered = input.len();
    }

    fn finalize_hex(mut self) -> String {
        let bit_len = self.total_bytes.wrapping_mul(8);
        self.buffer[self.buffered] = 0x80;
        self.buffered += 1;
        if self.buffered > 56 {
            self.buffer[self.buffered..].fill(0);
            let block = self.buffer;
            self.compress(&block);
            self.buffer = [0; 64];
        } else {
            self.buffer[self.buffered..56].fill(0);
        }
        self.buffer[56..64].copy_from_slice(&bit_len.to_be_bytes());
        let block = self.buffer;
        self.compress(&block);
        let mut output = String::with_capacity(64);
        for value in self.state {
            use std::fmt::Write as _;
            write!(&mut output, "{value:08x}").expect("写入 String 不会失败");
        }
        output
    }

    fn compress(&mut self, block: &[u8]) {
        const K: [u32; 64] = [
            0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
            0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
            0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
            0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
            0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
            0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
            0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
            0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
            0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
            0xc67178f2,
        ];
        let mut words = [0_u32; 64];
        for (index, chunk) in block.chunks_exact(4).take(16).enumerate() {
            words[index] = u32::from_be_bytes(chunk.try_into().expect("块长度固定"));
        }
        for index in 16..64 {
            let s0 = words[index - 15].rotate_right(7)
                ^ words[index - 15].rotate_right(18)
                ^ (words[index - 15] >> 3);
            let s1 = words[index - 2].rotate_right(17)
                ^ words[index - 2].rotate_right(19)
                ^ (words[index - 2] >> 10);
            words[index] = words[index - 16]
                .wrapping_add(s0)
                .wrapping_add(words[index - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = self.state;
        for index in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let choice = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(choice)
                .wrapping_add(K[index])
                .wrapping_add(words[index]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(majority);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        for (slot, value) in self.state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *slot = slot.wrapping_add(value);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::{FileHashCancellationToken, FileHashError, hash_reader};

    #[test]
    fn known_vectors_match_across_stream_chunks() {
        let token = FileHashCancellationToken::default();
        assert_eq!(
            hash_reader(io::Cursor::new(b""), &token).expect("空输入应可计算"),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        let input = vec![b'a'; 64 * 1024 + 37];
        assert_eq!(
            hash_reader(io::Cursor::new(input), &token).expect("跨缓冲区输入应可计算"),
            "be828e2ed4ae631a4de11778a9819b353d3bbcae31954f0a444b166052211c8c"
        );
    }

    #[test]
    fn cancellation_stops_before_reading() {
        let token = FileHashCancellationToken::default();
        token.cancel();
        assert_eq!(
            hash_reader(io::Cursor::new(b"secret"), &token),
            Err(FileHashError::Cancelled)
        );
    }

    #[test]
    fn read_errors_do_not_include_sensitive_input() {
        struct FailingReader;
        impl io::Read for FailingReader {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::other("C:\\private\\secret.mp4"))
            }
        }
        let error = hash_reader(FailingReader, &FileHashCancellationToken::default())
            .expect_err("读取失败必须返回脱敏错误");
        assert_eq!(error, FileHashError::ReadFailed);
        assert!(!error.to_string().contains("private"));
    }
}
