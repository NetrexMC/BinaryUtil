// #![feature(log_syntax)]

use std::any::type_name;
use std::convert::{From, Into, TryInto};
use std::io as std_io;
use std::net::{IpAddr, Ipv6Addr, SocketAddr, SocketAddrV6};

pub use bin_macro::*;

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use error::BinaryError;
use std::io::{Cursor, Read, Write};

/// Error utilities for Binary Utils.
/// This allows better handling of errors.
///
/// By default, errors **can** be converted to: `std::io::Error`
pub mod error;
pub mod io;
mod u24_impl;
pub mod varint;

pub use self::{u24_impl::*, varint::*};

macro_rules! includes {
    ($var: ident, $method: ident, $values: expr) => {{
        let v = &$values;
        v.iter().filter(|&v| $var.$method(v)).count() > 0
    }};
}

/// A trait to parse and unparse header structs from a given buffer.
///
/// **Example:**
/// ```rust
/// use binary_utils::{Streamable, error::BinaryError};
///
/// struct Foo {
///     bar: u8,
///     foo_bar: u16
/// }
/// impl Streamable for Foo {
///     fn parse(&self) -> Result<Vec<u8>, BinaryError> {
///         use std::io::Write;
///         let mut stream = Vec::<u8>::new();
///         stream.write_all(&self.bar.parse()?[..])?;
///         stream.write_all(&self.bar.parse()?[..])?;
///         Ok(stream)
///     }
///
///     fn compose(source: &[u8], position: &mut usize) -> Result<Self, BinaryError> {
///         // Streamable is implemented for all primitives, so we can
///         // just use this implementation to read our properties.
///         Ok(Self {
///             bar: u8::compose(&source, position)?,
///             foo_bar: u16::compose(&source, position)?
///         })
///     }
/// }
/// ```
pub trait Streamable {
    /// Writes `self` to the given buffer.
    fn parse(&self) -> Result<Vec<u8>, BinaryError>;

    /// Writes and unwraps `self` to the given buffer.
    ///
    /// ⚠️ This method is not fail safe, and will panic if result is Err.
    fn fparse(&self) -> Vec<u8> {
        self.parse().unwrap()
    }

    /// Reads `self` from the given buffer.
    fn compose(source: &[u8], position: &mut usize) -> Result<Self, BinaryError>
    where
        Self: Sized;

    /// Reads and unwraps `self` from the given buffer.
    ///
    /// ⚠️ This method is not fail safe, and will panic if result is Err.
    fn fcompose(source: &[u8], position: &mut usize) -> Self
    where
        Self: Sized,
    {
        Self::compose(source, position).unwrap()
    }
}

/// Little Endian Type
///
/// **Notice:**
/// This struct assumes the incoming buffer is BE and needs to be transformed.
///
/// For LE decoding in BE streams use:
/// ```rust
/// use binary_utils::{LE, Streamable, error::BinaryError};
///
/// fn read_u16_le(source: &[u8], offset: &mut usize) -> LE<u16> {
///     // get the size of your type, in this case it's 2 bytes.
///     let be_source = &source[*offset..2];
///     *offset += 2;
///     // now we can form the little endian
///     return LE::<u16>::compose(&be_source, &mut 0).unwrap();
/// }
///
/// assert_eq!(LE::<u16>(10).inner(), read_u16_le(&[10, 0], &mut 0).inner());
/// ```
#[derive(Debug, Clone, Copy)]
pub struct LE<T>(pub T);

impl<T> LE<T> {
    /// Grabs the `inner` type, similar to `unwrap`.
    pub fn inner(self) -> T {
        self.0
    }
}

impl<T> Streamable for LE<T>
where
    T: Streamable + Sized,
{
    fn parse(&self) -> Result<Vec<u8>, BinaryError> {
        let bytes = self.0.parse()?;
        Ok(reverse_vec(bytes))
    }

    fn compose(source: &[u8], position: &mut usize) -> Result<Self, BinaryError> {
        // If the source is expected to be LE we can swap it to BE bytes
        // Doing this makes the byte stream officially BE.
        // We actually need to do some hacky stuff here,
        // we need to get the size of `T` (in bytes)
        let stream = {
            // if we can get the value of the type we do so here.
            let name = type_name::<T>();

            if includes!(
                name,
                contains,
                [
                    "u8", "u16", "u32", "u64", "u128", "i8", "i16", "i32", "i64", "i128", "f32",
                    "f64"
                ]
            ) {
                reverse_vec(source[*position..(*position + ::std::mem::size_of::<T>())].to_vec())
            } else {
                reverse_vec(source[*position..].to_vec())
            }
        };

        // todo Properly implement LE streams
        // todo Get rid of this NASTY hack!
        // we need to get the stream releative to the current source, and "inject" into the current source.
        // we can do this by getting the position and the length of the stream.
        let mut hacked_stream = Vec::<u8>::new();
        let (q1, q2) = (
            hacked_stream.write_all(&source[..*position]),
            hacked_stream.write_all(&stream),
        );

        // check if any of the queries were invalid or failed.
        if q1.is_err() || q2.is_err() {
            Err(BinaryError::RecoverableKnown(
                "Write operation was interupted.".to_owned(),
            ))
        } else {
            Ok(LE(T::compose(&hacked_stream[..], position)?))
        }
    }
}

/// Reverses the bytes in a given vector
pub fn reverse_vec(bytes: Vec<u8>) -> Vec<u8> {
    let mut ret: Vec<u8> = Vec::new();

    for x in (0..bytes.len()).rev() {
        ret.push(*bytes.get(x).unwrap());
    }
    ret
}

/// Big Endian Encoding
pub struct BE<T>(pub T);

macro_rules! impl_streamable_primitive {
    ($ty: ty) => {
        impl Streamable for $ty {
            fn parse(&self) -> Result<Vec<u8>, BinaryError> {
                Ok(self.to_be_bytes().to_vec())
            }

            fn compose(source: &[u8], position: &mut usize) -> Result<Self, BinaryError> {
                // get the size
                let size = ::std::mem::size_of::<$ty>();
                let range = position.clone()..(size + position.clone());
                let data = <$ty>::from_be_bytes(source.get(range).unwrap().try_into().unwrap());
                *position += size;
                Ok(data)
            }
        }

        // impl Streamable for LE<$ty> {
        //     fn parse(&self) -> Vec<u8> {
        //         reverse_vec(self.0.parse())
        //     }

        //     fn compose(source: &[u8], position: &mut usize) -> Self {
        //         // If the source is expected to be LE we can swap it to BE bytes
        //         // Doing this makes the byte stream officially BE.
        //         // We actually need to do some hacky stuff here,
        //         // we need to get the size of `T` (in bytes)
        //         let stream = reverse_vec(source[*position..(*position + ::std::mem::size_of::<$ty>())].to_vec());
        //         LE(<$ty>::compose(&stream[..], position))
        //     }
        // }
    };
}

impl_streamable_primitive!(u8);
impl_streamable_primitive!(u16);
impl_streamable_primitive!(u32);
impl_streamable_primitive!(f32);
impl_streamable_primitive!(u64);
impl_streamable_primitive!(f64);
impl_streamable_primitive!(u128);
impl_streamable_primitive!(i8);
impl_streamable_primitive!(i16);
impl_streamable_primitive!(i32);
impl_streamable_primitive!(i64);
impl_streamable_primitive!(i128);

macro_rules! impl_streamable_vec_primitive {
    ($ty: ty) => {
        impl Streamable for Vec<$ty> {
            fn parse(&self) -> Result<Vec<u8>, BinaryError> {
                use ::std::io::Write;
                // write the length as a varint
                let mut v: Vec<u8> = Vec::new();
                v.write_all(&VarInt(v.len() as u32).to_be_bytes()[..])
                    .unwrap();
                for x in self.iter() {
                    v.extend(x.parse()?.iter());
                }
                Ok(v)
            }

            fn compose(source: &[u8], position: &mut usize) -> Result<Self, BinaryError> {
                // use ::std::io::Read;
                // read a var_int
                let mut ret: Vec<$ty> = Vec::new();
                let varint = VarInt::<u32>::from_be_bytes(source)?;
                let length: u32 = varint.into();

                *position += varint.get_byte_length() as usize;

                // read each length
                for _ in 0..length {
                    ret.push(<$ty>::compose(&source, position)?);
                }
                Ok(ret)
            }
        }
    };
}

impl_streamable_vec_primitive!(u8);
impl_streamable_vec_primitive!(u16);
impl_streamable_vec_primitive!(u32);
impl_streamable_vec_primitive!(f32);
impl_streamable_vec_primitive!(f64);
impl_streamable_vec_primitive!(u64);
impl_streamable_vec_primitive!(u128);
impl_streamable_vec_primitive!(i8);
impl_streamable_vec_primitive!(i16);
impl_streamable_vec_primitive!(i32);
impl_streamable_vec_primitive!(i64);
impl_streamable_vec_primitive!(i128);

// implements bools
impl Streamable for bool {
    fn parse(&self) -> Result<Vec<u8>, BinaryError> {
        Ok(vec![if *self { 1 } else { 0 }])
    }

    fn compose(source: &[u8], position: &mut usize) -> Result<Self, BinaryError> {
        // header validation
        if source[*position] > 1 {
            Err(BinaryError::RecoverableKnown(format!(
                "Tried composing binary from non-binary byte: {}",
                source[*position]
            )))
        } else {
            let v = source[*position] == 1;
            *position += 1;
            Ok(v)
        }
    }
}

impl Streamable for String {
    fn parse(&self) -> Result<Vec<u8>, BinaryError> {
        let mut buffer = Vec::<u8>::new();
        buffer.write_u16::<BigEndian>(self.len() as u16)?;
        buffer.write_all(self.as_bytes())?;
        Ok(buffer)
    }

    fn compose(source: &[u8], position: &mut usize) -> Result<Self, BinaryError> {
        let mut stream = Cursor::new(source);
        stream.set_position(position.clone() as u64);
        // Maybe do this in the future?
        let len: usize = stream.read_u16::<BigEndian>()?.into();
        *position = (stream.position() as usize) + len;

        unsafe {
            // todo: Remove this nasty hack.
            // todo: The hack being, remove the 2 from indexing on read_short
            // todo: And utilize stream.
            Ok(String::from_utf8_unchecked(
                stream.get_ref()[2..len + stream.position() as usize].to_vec(),
            ))
        }
    }
}

impl Streamable for SocketAddr {
    fn parse(&self) -> Result<Vec<u8>, BinaryError> {
        let mut stream = Vec::<u8>::new();
        match *self {
            Self::V4(_) => {
                stream.write_u8(4)?;
                let partstr = self.to_string();
                let actstr = partstr.split(":").collect::<Vec<&str>>()[0];
                let parts: Vec<&str> = actstr.split(".").collect();
                for part in parts {
                    let mask = part.parse::<u8>().unwrap_or(0);
                    stream.write_u8(mask)?;
                }
                stream
                    .write_u16::<BigEndian>(self.port())
                    .expect("Could not write port to stream.");
                Ok(stream)
            }
            Self::V6(addr) => {
                stream.write_u8(6)?;
                // family? or length??
                stream.write_u16::<BigEndian>(0)?;
                // port
                stream.write_u16::<BigEndian>(self.port())?;
                // flow
                stream.write_u32::<BigEndian>(addr.flowinfo())?;
                // actual address here
                stream.write(&addr.ip().octets())?;
                // scope
                stream.write_u32::<BigEndian>(addr.scope_id())?;
                Ok(stream)
            }
        }
    }

    fn compose(source: &[u8], position: &mut usize) -> Result<Self, BinaryError> {
        let mut stream = Cursor::new(source);
        stream.set_position(*position as u64);
        match stream.read_u8()? {
            4 => {
                let from = stream.position() as usize;
                let to = stream.position() as usize + 4;
                let parts = &source[from..to];
                stream.set_position(to as u64);
                let port = stream.read_u16::<BigEndian>().unwrap();
                *position = stream.position() as usize;
                Ok(SocketAddr::new(
                    IpAddr::from([parts[0], parts[1], parts[2], parts[3]]),
                    port,
                ))
            }
            6 => {
                let _family = stream.read_u16::<BigEndian>().unwrap();
                let port = stream.read_u16::<BigEndian>().unwrap();
                let flow = stream.read_u32::<BigEndian>().unwrap();
                let mut parts: [u8; 16] = [0; 16];
                stream.read(&mut parts).unwrap();
                // we need to read parts into address
                let address = {
                    let mut s = Cursor::new(parts);
                    let (a, b, c, d, e, f, g, h) = (
                        s.read_u16::<BigEndian>().unwrap_or(0),
                        s.read_u16::<BigEndian>().unwrap_or(0),
                        s.read_u16::<BigEndian>().unwrap_or(0),
                        s.read_u16::<BigEndian>().unwrap_or(0),
                        s.read_u16::<BigEndian>().unwrap_or(0),
                        s.read_u16::<BigEndian>().unwrap_or(0),
                        s.read_u16::<BigEndian>().unwrap_or(0),
                        s.read_u16::<BigEndian>().unwrap_or(0),
                    );
                    Ipv6Addr::new(a, b, c, d, e, f, g, h)
                };
                let scope = stream.read_u32::<BigEndian>().unwrap();
                *position = stream.position() as usize;
                Ok(SocketAddr::from(SocketAddrV6::new(
                    address, port, flow, scope,
                )))
            }
            _ => panic!("Unknown Address type!"),
        }
    }
}

/// Writes a vector whose length is written with a short
impl<T> Streamable for Vec<LE<T>>
where
    T: Streamable,
{
    fn parse(&self) -> Result<Vec<u8>, BinaryError> {
        // write the length as a varint
        let mut v: Vec<u8> = Vec::new();
        v.write_u16::<BigEndian>(self.len() as u16)?;
        for x in self.iter() {
            v.extend(x.parse()?.iter());
        }
        Ok(v)
    }

    fn compose(source: &[u8], position: &mut usize) -> Result<Self, BinaryError> {
        // read a var_int
        let mut stream = Cursor::new(source);
        let mut ret: Vec<LE<T>> = Vec::new();
        let length = stream.read_u16::<BigEndian>()?;
        *position = stream.position() as usize;
        // read each length
        for _ in 0..length {
            ret.push(LE::<T>::compose(&source[*position..], &mut 0)?);
        }
        Ok(ret)
    }
}
