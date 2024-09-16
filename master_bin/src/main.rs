
use nb::block;
use serde::{Deserialize, Serialize};
use serde_derive;
use serde_hex;
use defaults::Defaults;
use serde_hex::{SerHex,StrictPfx};
use tokio::{self, task};
use mini_redis::{client, Result};
use core::result::Result as ResultC;
use std::{thread, time::Duration};
use futures::{stream::StreamExt, SinkExt};
use futures_channel::mpsc;
use tokio_serial::{SerialPort, SerialPortBuilderExt, StopBits};
use tokio_util::codec::{Decoder, Encoder};
use masterapi::{self, LineCodec};

use linux_embedded_hal::I2cdev;
use ads1x1x::{channel, Ads1x1x, ChannelSelection, DataRate16Bit, DynamicOneShot, ModeChangeError, SlaveAddr};
extern crate embedded_hal;



#[cfg(unix)]
// const SERIAL_DEVICE: &'static str = env!("SERIAL_DEVICE");
const SERIAL_DEVICE: &'static str = "/dev/ttyAMA4";

const ADDR_ADS115:     u16 = 0x48; // Address of first ADS115 chip  
const DELAY_TIME:        u64 = 200; 


#[tokio::main]
async fn main() -> Result<()> {
    // let dev = I2cdev::new("/dev/i2c-1").unwrap();
    let dev = I2cdev::new("/dev/i2c-1").unwrap();
    let address = SlaveAddr::default();
    let mut adc = Ads1x1x::new_ads1115(dev, address);
    adc.set_data_rate(DataRate16Bit::Sps860).unwrap();

    let mut port = tokio_serial::new(SERIAL_DEVICE, 115200).open_native_async().unwrap();
    #[cfg(unix)]
    port.set_stop_bits(StopBits::One).unwrap();
    let (mut writer, mut reader) = LineCodec.framed(port).split();
    task::spawn(async move{
        #[cfg(unix)]
            // port.set_stop_bits(StopBits::One).unwrap();
            // let mut reader =LineCodec.framed(port);
        while let Some(line_result) = reader.next().await {
            if let Ok(line)=line_result{
                println!("{:?}",line)
            }
        }
    });
    // let mut i2c = I2c::new().unwrap();
    // i2c.set_slave_address(ADDR_ADS115).unwrap();

    // let mut reg = [0u8; 2];
    // i2c.block_read(0x00, &mut reg).unwrap(); 
    // thread::sleep(Duration::from_millis(DELAY_TIME));

    
    loop{
        println!("Main Loop");
        let measurement = block!(adc.read(ChannelSelection::DifferentialA0A1)).unwrap();
        println!("Measurement: {}", measurement);
        thread::sleep(Duration::from_millis(500));
        
    }

    Ok(())
}