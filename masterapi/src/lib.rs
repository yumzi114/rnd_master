

use core::str;
use std::io::{self, Error, ErrorKind};

use bytes::{BufMut, BytesMut};
use tokio_util::codec::{Decoder, Encoder};


#[cfg(unix)]
// const SERIAL_DEVICE: &'static str = env!("SERIAL_DEVICE");
const SERIAL_DEVICE: &'static str = "/dev/ttyAMA4";
#[cfg(windows)]
const DEFAULT_TTY: &str = "COM1";


pub struct LineCodec;

impl Decoder for LineCodec {
    type Item = Vec<u8>;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        // let newline = src.as_ref().iter().position(|b| *b == b'j');
        let start = src.as_ref().iter().position(|x| *x == 0xFC);
        if let Some(n) = start {
            let line = src.split_to(n+1);
            let line_list = line.to_vec();
            if line_list.len()==6&&line_list[0]==0xAF&&line_list[1]==3{
                // if line_list[3]==00{
                //     return Err(Error::other("Device S/N Error"));
                // }
                return Ok(Some(line_list));
            }
            else {
                return Ok(None)
                // return Err(Error::new(ErrorKind::NotConnected, "Device Not Connected"));
            }

        }
        // if let Some(n) = newline {
        //     let line = src.split_to(n + 1);
        //     return match str::from_utf8(line.as_ref()) {
        //         Ok(s) => Ok(Some(s.to_string())),
        //         Err(_) => Err(io::Error::new(io::ErrorKind::Other, "Invalid String")),
        //     };
        // }
        Ok(None)
    }
}

impl Encoder<Vec<u8>> for LineCodec {
    type Error = io::Error;

    fn encode(&mut self, item: Vec<u8>, buf: &mut BytesMut) -> Result<(), Self::Error> {
        
        for i in item{
            buf.put_u8(i);
        }
        
        Ok(())
    }
}