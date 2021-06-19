use std::convert::TryInto;
use std::string::FromUtf8Error;
use std::ops::{ Range, Index, IndexMut };

use super::buffer;

pub struct BinaryStream {
     buffer: Vec<u8>,
     offset: usize,
     bounds: (usize, usize)
}

impl BinaryStream {
     /// Increases the offset. If `None` is given in `amount`, 1 will be used.
     fn increase_offset(&mut self, amount: Option<usize>) -> usize {
          let amnt = match amount {
               None => 1 as usize,
               Some(n) => n
          };

          if (self.offset + amnt) > self.bounds.1 {
               panic!("Offset outside buffer.");
          }

          self.offset = self.offset + amnt;
          self.offset
     }

     /// Changes the offset of the stream to the new given offset.
     /// returns `true` if the offset is in bounds and `false` if the offset is out of bounds.
     fn set_offset(&mut self, offset: usize) -> bool {
          if offset > self.bounds.1 {
               false
          } else {
               self.offset = offset;
               true
          }
     }

     /// Returns the current offset at the given time when called.
     fn get_offset(&mut self) -> usize {
          self.offset
     }

     /// Allocates more bytes to the binary stream.
     /// Allocations can occur as many times as desired, however a negative allocation will cause
     /// the stream to "drop" or "delete" bytes from the buffer. Discarded bytes are not recoverable.
     ///
     /// Useful when writing to a stream, allows for allocating for chunks, etc.
     ///
     /// **Example:**
     ///
     ///     stream.allocate(1024);
     ///     stream.write_string(String::from("a random string, that can only be a max of 1024 bytes."));
     fn allocate(&mut self, bytes: usize) {
          self.bounds.1 = self.buffer.len() + bytes;
          self.buffer.resize(self.bounds.1, 0)
     }

     /// Create a new Binary Stream from a vector of bytes.
     fn new(buf: &Vec<u8>) -> Self {
          Self {
               buffer: buf.clone(),
               bounds: (0, buf.len()),
               offset: 0
          }
     }

     /// Similar to slice, clamp, "grips" the buffer from a given offset, and changes the initial bounds.
     /// Meaning that any previous bytes before the given bounds are no longer writable.
     ///
     /// Useful for cloning "part" of a stream, and only allowing certain "bytes" to be read.
     /// Clamps can not be undone.
     ///
     /// **Example:**
     ///
     ///     let stream = BinaryStream::new(vec!(([98,105,110,97,114,121,32,117,116,105,108,115]));
     ///     let shareable_stream = stream.clamp(7); // 32,117,116,105,108,115 are now the only bytes readable externally
     fn clamp(&mut self, offset: usize) -> Self {
          // makes sure that the bound is still possible
          if offset > self.buffer.len() {
               panic!("Bounds not possible");
          } else {
               self.bounds.0 = offset;
               BinaryStream::new(&mut self.buffer.clone()) // Dereferrenced for use by consumer.
          }
     }

     /// Checks whether or not the given offset is in between the streams bounds and if the offset is valid.
     ///
     /// **Example:**
     ///
     ///     if stream.is_within_bounds(100) {
     ///       println!("Can write to offset: 100");
     ///     } else {
     ///       println!("100 is out of bounds.");
     ///     }
     fn is_within_bounds(&self, offset: usize) -> bool {
          !(offset > self.bounds.1 || offset < self.bounds.0 || offset > self.buffer.len())
     }

     /// Reads a byte, updates the offset, clamps to last offset.
     ///
     /// **Example:**
     ///
     ///      let mut fbytes = Vec::new();
     ///      loop {
     ///         if fbytes.len() < 4 {
     ///           fbytes.push(stream.read());
     ///         }
     ///         break;
     ///      }
     fn read(&mut self) -> u8 {
          let byte = self[self.offset];
          self.clamp(self.offset);
          self.increase_offset(None);
          byte
     }
}

/// Implements indexing on BinaryStream.
/// When indexing you can access the bytes only readable by the streams bounds.
/// If the offset you're trying to index is "outside" of the "bounds" of the stream this will panic.
///
/// **Example:**
///
///     let first_byte = stream[0];
impl std::ops::Index<usize> for BinaryStream {
     type Output = u8;
     fn index(&self, idx: usize) -> &u8 {
          if !self.is_within_bounds(idx) {
               if self.bounds.0 == 0 && self.bounds.1 == self.buffer.len() {
                    panic!("Index is out of bounds due to clamp.");
               } else {
                    panic!("Index is out of bounds.");
               }
          }

          self.buffer.get(idx).unwrap()
     }
}

/// Implements indexing with slices on BinaryStream.
/// Operates exactly like indexing, except with slices.
///
/// **Example:**
///
///     let first_bytes = stream[0..3];
impl Index<Range<usize>> for BinaryStream {
     type Output = [u8];
     fn index(&self, idx: Range<usize>) -> &[u8] {
          if !self.is_within_bounds(idx.end) || !self.is_within_bounds(idx.start) {
               if self.bounds.0 == 0 && self.bounds.1 == self.buffer.len() {
                    panic!("Index is out of bounds due to clamp.");
               } else {
                    panic!("Index is out of bounds.");
               }
          }

          self.buffer.get(idx).unwrap()
     }
}

impl std::ops::IndexMut<usize> for BinaryStream {
     fn index_mut(&mut self, offset: usize) -> &mut u8 {
          if !self.is_within_bounds(offset) {
               self.buffer.get_mut(offset).unwrap()
          } else {
               panic!("Offset: {} is out of bounds.", offset);
          }
     }
}

impl buffer::IBufferRead for BinaryStream {
     /// Literally, reads a byte
     fn read_byte(&mut self) -> u16 {
          let idx = self.offset;
          let unt = self.offset + 2;
          let byte = u16::from_be_bytes(self.buffer[idx..unt].try_into().unwrap());
          self.increase_offset(Some(2));
          byte
     }

     fn read_signed_byte(&mut self) -> i16 {
          let b = i16::from_be_bytes(self.buffer[self.offset..self.offset + 2].try_into().unwrap());
          self.increase_offset(Some(2));
          b
     }

     fn read_bool(&mut self) -> bool {
          self.read_byte() != 0
     }

     fn read_string(&mut self) -> Result<String, FromUtf8Error> {
          let length = self.read_short();
          let string = String::from_utf8(self[self.offset..self.offset + length as usize].to_vec());
          self.increase_offset(Some(self.offset + length as usize));
          string
     }

     fn read_short(&mut self) -> u16 {
          // a short is 2 bytes and is a u16,
          // this is essentially just "read_byte"
          self.read_byte()
     }

     fn read_signed_short(&mut self) -> i16 {
          self.read_signed_byte()
     }

     fn read_short_le(&mut self) -> u16 {
          let b = u16::from_le_bytes(self.buffer[self.offset..self.offset + 2].try_into().unwrap());
          self.increase_offset(Some(2));
          b
     }

     fn read_signed_short_le(&mut self) -> i16 {
          let b = i16::from_le_bytes(self.buffer[self.offset..self.offset + 2].try_into().unwrap());
          self.increase_offset(Some(2));
          b
     }

     fn read_triad(&mut self) -> usize {
          // a triad is 3 bytes
          // let b = u32::from_be_bytes(self[self.offset..self.offset + 3].try_into().unwrap());
          // b
          0
     }

     fn read_triad_le(&mut self) -> usize {
          0
     }

     fn read_int(&mut self) -> i16 {
          self.read_signed_short()
     }


     fn read_int_le(&mut self) -> i16 {
          self.read_signed_short_le()
     }

     fn read_float(&mut self) -> f32 {
          let b = f32::from_be_bytes(self.buffer[self.offset..self.offset + 2].try_into().unwrap());
          self.increase_offset(Some(2));
          b
     }

     fn read_float_le(&mut self) -> f32 {
          let b = f32::from_le_bytes(self.buffer[self.offset..self.offset + 2].try_into().unwrap());
          self.increase_offset(Some(2));
          b
     }

     fn read_double(&mut self) -> f64 {
          let b = f64::from_be_bytes(self.buffer[self.offset..self.offset + 2].try_into().unwrap());
          self.increase_offset(Some(2));
          b
     }

     fn read_double_le(&mut self) -> f64 {
          let b = f64::from_le_bytes(self.buffer[self.offset..self.offset + 2].try_into().unwrap());
          self.increase_offset(Some(2));
          b
     }

     fn read_long(&mut self) -> i64 {
          let b = i64::from_be_bytes(self.buffer[self.offset..self.offset + 2].try_into().unwrap());
          self.increase_offset(Some(2));
          b
     }

     fn read_long_le(&mut self) -> i64 {
          let b = i64::from_le_bytes(self.buffer[self.offset..self.offset + 2].try_into().unwrap());
          self.increase_offset(Some(2));
          b
     }

     fn read_var_int(&mut self) -> isize {
          // taken from pmmp, this might be messed up
          let mut b: i16 = 0;
          let mut i = 0;
          while i <= 28 {
               let byte = self.read_signed_byte();
               b |= (byte & 0x7f) << i;
               if (byte & 0x80) == 0 {
                    return b as isize
               }
               i += 7;
          }
          return b as isize;
     }

     fn read_signed_var_int(&mut self) -> isize {
          0
     }

     fn read_var_long(&mut self) -> isize {
          0
     }

     fn read_signed_var_long(&mut self) -> isize {
          0
     }
}