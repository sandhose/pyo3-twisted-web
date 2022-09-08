// Copyright 2022 Quentin Gliech
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use bytes::Buf;

/// A boxed `Buf`
pub struct BoxBuf {
    inner: Box<dyn Buf>,
}

impl BoxBuf {
    pub fn new<B: Buf + 'static>(buf: B) -> Self {
        Self {
            inner: Box::new(buf),
        }
    }
}

impl Buf for BoxBuf {
    fn remaining(&self) -> usize {
        self.inner.remaining()
    }

    fn chunk(&self) -> &[u8] {
        self.inner.chunk()
    }

    fn advance(&mut self, cnt: usize) {
        self.inner.advance(cnt);
    }

    fn chunks_vectored<'a>(&'a self, dst: &mut [std::io::IoSlice<'a>]) -> usize {
        self.inner.chunks_vectored(dst)
    }

    fn has_remaining(&self) -> bool {
        self.inner.has_remaining()
    }

    fn copy_to_slice(&mut self, dst: &mut [u8]) {
        self.inner.copy_to_slice(dst);
    }

    fn get_u8(&mut self) -> u8 {
        self.inner.get_u8()
    }

    fn get_i8(&mut self) -> i8 {
        self.inner.get_i8()
    }

    fn get_u16(&mut self) -> u16 {
        self.inner.get_u16()
    }

    fn get_u16_le(&mut self) -> u16 {
        self.inner.get_u16_le()
    }

    fn get_i16(&mut self) -> i16 {
        self.inner.get_i16()
    }

    fn get_i16_le(&mut self) -> i16 {
        self.inner.get_i16_le()
    }

    fn get_u32(&mut self) -> u32 {
        self.inner.get_u32()
    }

    fn get_u32_le(&mut self) -> u32 {
        self.inner.get_u32_le()
    }

    fn get_i32(&mut self) -> i32 {
        self.inner.get_i32()
    }

    fn get_i32_le(&mut self) -> i32 {
        self.inner.get_i32_le()
    }

    fn get_u64(&mut self) -> u64 {
        self.inner.get_u64()
    }

    fn get_u64_le(&mut self) -> u64 {
        self.inner.get_u64_le()
    }

    fn get_i64(&mut self) -> i64 {
        self.inner.get_i64()
    }

    fn get_i64_le(&mut self) -> i64 {
        self.inner.get_i64_le()
    }

    fn get_u128(&mut self) -> u128 {
        self.inner.get_u128()
    }

    fn get_u128_le(&mut self) -> u128 {
        self.inner.get_u128_le()
    }

    fn get_i128(&mut self) -> i128 {
        self.inner.get_i128()
    }

    fn get_i128_le(&mut self) -> i128 {
        self.inner.get_i128_le()
    }

    fn get_uint(&mut self, nbytes: usize) -> u64 {
        self.inner.get_uint(nbytes)
    }

    fn get_uint_le(&mut self, nbytes: usize) -> u64 {
        self.inner.get_uint_le(nbytes)
    }

    fn get_int(&mut self, nbytes: usize) -> i64 {
        self.inner.get_int(nbytes)
    }

    fn get_int_le(&mut self, nbytes: usize) -> i64 {
        self.inner.get_int_le(nbytes)
    }

    fn get_f32(&mut self) -> f32 {
        self.inner.get_f32()
    }

    fn get_f32_le(&mut self) -> f32 {
        self.inner.get_f32_le()
    }

    fn get_f64(&mut self) -> f64 {
        self.inner.get_f64()
    }

    fn get_f64_le(&mut self) -> f64 {
        self.inner.get_f64_le()
    }

    fn copy_to_bytes(&mut self, len: usize) -> bytes::Bytes {
        self.inner.copy_to_bytes(len)
    }
}
