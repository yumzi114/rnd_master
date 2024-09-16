
use nb::block;
use serde::{Deserialize, Serialize};
use serde_derive;
use serde_hex;
use defaults::Defaults;
use serde_hex::{SerHex,StrictPfx};
use tokio::{self, task};
use mini_redis::{client, Result};
use core::result::Result as ResultC;
use std::{borrow::{Borrow, BorrowMut}, sync::{Arc, Mutex}, thread, time::Duration};
use futures::{stream::StreamExt, SinkExt};
use futures_channel::mpsc;
use tokio_serial::{SerialPort, SerialPortBuilderExt, StopBits};
use tokio_util::codec::{Decoder, Encoder};
use masterapi::{self, LineCodec};
use tracing::{info, trace, warn, error};
use tracing_subscriber;
use linux_embedded_hal::I2cdev;
use ads1x1x::{channel, Ads1x1x, ChannelSelection, ComparatorLatching, ComparatorMode, ComparatorPolarity, ComparatorQueue, DataRate16Bit, DynamicOneShot, FullScaleRange, ModeChangeError, SlaveAddr};
extern crate embedded_hal;



#[cfg(unix)]
// const SERIAL_DEVICE: &'static str = env!("SERIAL_DEVICE");
const SERIAL_DEVICE: &'static str = "/dev/ttyAMA4";

#[tokio::main]
async fn main() -> Result<()> {
    tracing::subscriber::set_global_default(
        tracing_subscriber::FmtSubscriber::new()
    ).expect("setting default subscriber failed");
    //=============ADS1115 변환기 설정=============
    let dev = I2cdev::new("/dev/i2c-1").unwrap();
    let address = SlaveAddr::default();
    let mut adc = Ads1x1x::new_ads1115(dev, address);
    //=============ADS1115 속도=============
    adc.set_data_rate(DataRate16Bit::Sps860).unwrap();
    //=============ADS1115 전압이 -1.5V 이하로 떨어지거나 최소 두 번의 연속 변환에서 1.5V 이상으로 올라갈 때 비교기를 구성=============
    adc.set_comparator_queue ( ComparatorQueue::Two ) .unwrap ( ) ;
    adc.set_comparator_polarity ( ComparatorPolarity::ActiveHigh ).unwrap ( ) ;
    adc.set_comparator_mode ( ComparatorMode :: Window ) .unwrap ();
    adc.set_full_scale_range ( FullScaleRange :: Within2_048V ) .unwrap ( ) ;
    adc.set_low_threshold_raw ( -1500 ) .unwrap ( ) ;
    adc.set_high_threshold_raw ( 1500 ) . unwrap ();
    adc.set_comparator_latching ( ComparatorLatching::Latching ) .unwrap ( ) ; 
    //=============ADS1115 공유메모리=============
    let arc_adc = Arc::new(Mutex::new(adc));
    //=============시리얼설정=============
    let mut port = tokio_serial::new(SERIAL_DEVICE, 115200).open_native_async().unwrap();
    #[cfg(unix)]
    port.set_stop_bits(StopBits::One).unwrap();
    let (mut writer, mut reader) = LineCodec.framed(port).split();
    //=============시리얼 리더 스레드=============
    task::spawn(async move{
        #[cfg(unix)]
            // port.set_stop_bits(StopBits::One).unwrap();
            // let mut reader =LineCodec.framed(port);
        trace!("Serial Port Open Device : {:?}",SERIAL_DEVICE);
        while let Some(line_result) = reader.next().await {
            if let Ok(line)=line_result{
                info!("READ [REQ]: {:?}", line);
                println!("{:?}",line)
            }
        }
    });
    //=============센서1 측정=============
    let arc_mem = arc_adc.clone();
    task::spawn(async move{
        #[cfg(unix)]
        loop{
            let senser1 = block!(arc_mem.lock().unwrap().read(ChannelSelection::SingleA1)).unwrap();
            match senser1 {
                -32768..250=>{
                    
                },
                _=>{
                    let mut list = vec![];
                    for i in 0..10{
                        thread::sleep(Duration::from_millis(1));
                        list.push(
                            block!(arc_mem.lock().unwrap().read(ChannelSelection::SingleA1)).unwrap()
                        );
                    }
                    let all_check = list.iter().all(|&x|x > 250);
                    if all_check {
                        info!("READ [SENSER1]: DOWN");
                        thread::sleep(Duration::from_millis(1000));
                    }
                    else {
                        continue;
                    }
                    
                }
            }
            thread::sleep(Duration::from_millis(1));
        }
    });
    //=============센서2 측정=============
    let arc_mem = arc_adc.clone();
    task::spawn(async move{
        #[cfg(unix)]
        loop{
            let senser2 = block!(arc_mem.lock().unwrap().read(ChannelSelection::SingleA2)).unwrap();
            match senser2 {
                -32768..250=>{
                    
                },
                _=>{
                    info!("READ [SENSER2]: {:?}", senser2);
                }
            }
            thread::sleep(Duration::from_millis(1));
        }
    });
    
    loop{
        thread::sleep(Duration::from_millis(50));
    }
    Ok(())
}