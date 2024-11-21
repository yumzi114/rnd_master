
use nb::block;
use serde_derive;
use serde_hex;
use serde_hex::{SerHex,StrictPfx};
use tokio::runtime::Runtime;
use tokio::{self, task};
use mini_redis::{client, Result};
use core::result::Result as ResultC;
use std::{borrow::{Borrow, BorrowMut}, sync::{Arc, Mutex}, thread, time::Duration};
use futures::{stream::StreamExt, SinkExt};
use futures_channel::mpsc;
use tokio_serial::{SerialPort, SerialPortBuilderExt, StopBits};
use tokio_util::codec::{Decoder, Encoder};
use masterapi::{self, LineCodec,Packet};
use tracing::{info, trace, warn, error};
use tracing_subscriber;
use linux_embedded_hal::I2cdev;
use ads1x1x::{channel, Ads1x1x, ChannelSelection, ComparatorLatching, ComparatorMode, ComparatorPolarity, ComparatorQueue, DataRate16Bit, DynamicOneShot, FullScaleRange, ModeChangeError, SlaveAddr};
use serde_derive::{Serialize, Deserialize};
use rppal::gpio::Gpio;
use crossbeam_channel::unbounded;

extern crate embedded_hal;


const MORTOR_RUN: u8 = 23;
const MORTOR_DIR: u8 = 24;
#[derive(Clone,Copy,PartialEq,Debug)]
enum SIGN {
    RED,
    GREEN
}


#[cfg(unix)]
// const SERIAL_DEVICE: &'static str = env!("SERIAL_DEVICE");
const SERIAL_DEVICE: &'static str = "/dev/ttyAMA4";

#[tokio::main]
async fn main() -> Result<()> {
    tracing::subscriber::set_global_default(
        tracing_subscriber::FmtSubscriber::new()
    ).expect("setting default subscriber failed");
    let mut moto_run_pin =Gpio::new().unwrap().get(MORTOR_RUN).unwrap().into_output();
    let mut moto_dir_pin =Gpio::new().unwrap().get(MORTOR_DIR).unwrap().into_output();
    let moto_run = Arc::new(Mutex::new(moto_run_pin));
    let moto_dir = Arc::new(Mutex::new(moto_dir_pin));
    let mut sign:Option<SIGN> = None;
    let app_sign = Arc::new(Mutex::new(sign));
    // 신호 채널
    let (s, r) = unbounded();
    //=============패킷 기본설정=============
    let mut app_report = Packet::default();
    app_report.command = 0x03;
    app_report.save("Report");
    let report = Arc::new(Mutex::new(app_report));

    let mut app_request = Packet::default();
    let request = Arc::new(Mutex::new(app_request));
    app_request.save("Request");
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
    //=============센서상태 공유=============
    let senser1_state = Arc::new(Mutex::new(false));
    let senser2_state = Arc::new(Mutex::new(false));
    let moto_state = Arc::new(Mutex::new(false));
    //=============시리얼설정=============
    let mut port = tokio_serial::new(SERIAL_DEVICE, 115200).open_native_async().unwrap();
    #[cfg(unix)]
    port.set_stop_bits(StopBits::One).unwrap();
    let (mut writer, mut reader) = LineCodec.framed(port).split();
    //=============시리얼 센더 스레드=============
    let report_mem = report.clone();
    thread::spawn(move||{
        let rt  = Runtime::new().unwrap();
        rt.block_on(async {
            trace!("Serial Port Sender Open Device : {:?}",SERIAL_DEVICE);
            loop{
                let mut packet = *report_mem.lock().unwrap();
                if let Ok(_)=packet.add_checksum(){
                    if let Ok(_)=packet.is_checksum(){
                        if let Ok(file_data)=confy::load("master_app", Some("Report")){
                            if packet !=file_data{
                                if let Ok(_)=writer.send(packet).await{
                                    info!("SEND [REPORT]: {:?}",packet);
                                }
                            }
                        }
                    }
                };
                thread::sleep(Duration::from_millis(50));
            }
        });
        
        
    });
    //=============시리얼 리더 스레드=============
    let moto_state_mem = moto_state.clone();
    let report_mem = report.clone();
    let moto_run_mem = moto_run.clone();
    let moto_dir_mem = moto_dir.clone();
    let app_sign_mem = app_sign.clone();
    thread::spawn(move||{
        let rt  = Runtime::new().unwrap();
        rt.block_on(async {
            trace!("Serial Port Reader Open Device : {:?}",SERIAL_DEVICE);
            while let Some(line_result) = reader.next().await {
                if let Ok(packet)=line_result{
                    match packet.command {
                        0x1=>{
                            let mut flag = 0;
                            if flag ==0 {
                                flag=1;
                                info!("READ [REQUEST]: {:?}",packet);
                                match packet.remote {
                                    0b0000_0001 =>{
                                        *app_sign_mem.lock().unwrap() =Some(SIGN::GREEN);
                                        if !moto_run_mem.lock().unwrap().is_set_high(){
                                            moto_run_mem.lock().unwrap().set_high();

                                        }
                                        if moto_dir_mem.lock().unwrap().is_set_high(){
                                            moto_dir_mem.lock().unwrap().set_low();
                                        }
                                        //센서 무시시간
                                        thread::sleep(Duration::from_millis(1500));
                                        *app_sign_mem.lock().unwrap() =None;
                                        flag=0;
                                    }
                                    0b0000_0010 =>{
                                        *app_sign_mem.lock().unwrap() =Some(SIGN::RED);
                                        if !moto_dir_mem.lock().unwrap().is_set_high(){
                                            moto_dir_mem.lock().unwrap().set_high();
                                        }
                                        if !moto_run_mem.lock().unwrap().is_set_high(){
                                            moto_run_mem.lock().unwrap().set_high();
                                        }
                                        //센서 무시시간
                                        thread::sleep(Duration::from_millis(1500));
                                        *app_sign_mem.lock().unwrap() =None;
                                        flag=0;
                                        
                                    }
                                    _=>{}
                                }
                            }
                            else {
                                flag=0;
                            }
                            
                        }
                        0x2=>{
                            info!("READ [RESPONSE]: {:?}",packet);
                        }
                        0x3=>{
                            info!("READ [REPORT]: {:?}",packet);
                        }
                        _=>{}
                    }
                    
                }
            }
        });
    });
    //=============센서1 측정=============
    let arc_mem = arc_adc.clone();
    let senser_mem = senser1_state.clone();
    let report_mem = report.clone();
    
    thread::spawn(move||{
        let rt  = Runtime::new().unwrap();
        rt.block_on(async {
            loop{

                let senser1 = block!(arc_mem.lock().unwrap().read(ChannelSelection::SingleA1)).unwrap();
                if senser1 != report_mem.lock().unwrap().pannel_up{
                    report_mem.lock().unwrap()                            // thread::sleep(Duration::from_millis(1000));
                    .pannel_up = senser1;
                    report_mem.lock().unwrap().save("Report");
                }
                match senser1 {
                    -32768..500=>{
                        let mut list = vec![];
                        for i in 0..5{
                            thread::sleep(Duration::from_millis(1));
                            list.push(
                                block!(arc_mem.lock().unwrap().read(ChannelSelection::SingleA1)).unwrap()
                            );
                        }
                        let all_check = list.iter().all(|&x|x < 500);
                        if all_check {
                            *senser_mem.lock().unwrap()=false;
                            let mut state = report_mem.lock().unwrap().sensor_state;
                            state &= 0b1111_1110;
                            report_mem.lock().unwrap().sensor_state = state;
                            report_mem.lock().unwrap().save("Report");
                            s.send(false).unwrap();
                            list.clear();
                            continue;
                        }
                        else {
                            
                            list.clear();
                            continue;
                        }
                        
                    },
                    _=>{
                        let mut list = vec![];
                        for i in 0..5{
                            thread::sleep(Duration::from_millis(1));
                            list.push(
                                block!(arc_mem.lock().unwrap().read(ChannelSelection::SingleA1)).unwrap()
                            );
                        }
                        let all_check = list.iter().all(|&x|x > 500);
                        if all_check {
                            *senser_mem.lock().unwrap()=true;
                            let mut state = report_mem.lock().unwrap().sensor_state;
                            state |= 0b0000_0001;
                            report_mem.lock().unwrap().sensor_state = state;
                            report_mem.lock().unwrap().save("Report");
                            list.clear();
                            continue;
                        }
                        else {
                            list.clear();
                            continue;
                        }
                    }
                }
                thread::sleep(Duration::from_millis(1));
            }
        });
        // #[cfg(unix)]
        
    });
    // //=============센서2 측정=============
    // let arc_mem = arc_adc.clone();
    // let senser_mem = senser2_state.clone();
    // let report_mem = report.clone();
    // thread::spawn(move||{app_sign_mem
    //     let rt  = Runtime::new().unwrap();
    //     rt.block_on(async {
    //         loop{
    //             let senser2 = block!(arc_mem.lock().unwrap().read(ChannelSelection::SingleA2)).unwrap();
    //             if senser2 != report_mem.lock().unwrap().pannel_down{
    //                 report_mem.lock().unwrap().pannel_down = senser2;
    //                 report_mem.lock().unwrap().save("Report");
    //             }
    //             match senser2 {
    //                 -32768..500=>{
    //                     let mut list = vec![];
    //                     for i in 0..15{
    //                         thread::sleep(Duration::from_millis(1));
    //                         list.push(
    //                             block!(arc_mem.lock().unwrap().read(ChannelSelection::SingleA2)).unwrap()
    //                         );
    //                     }
    //                     let all_check = list.iter().all(|&x|x < 500);
    //                     if all_check {
    //                         *senser_mem.lock().unwrap()=false;
    //                         let mut state = report_mem.lock().unwrap().sensor_state;
    //                         state &= 0b1111_1101;
    //                         report_mem.lock().unwrap().sensor_state = state;
    //                         report_mem.lock().unwrap().save("Report");
    //                         list.clear();
    //                         continue;
    //                     }
    //                     else {
    //                         list.clear();
    //                         continue;
    //                     }
                        
    //                 },
    //                 _=>{
    //                     let mut list = vec![];
    //                     for i in 0..15{
    //                         thread::sleep(Duration::from_millis(1));app_sign_mem
    //                         list.push(
    //                             block!(arc_mem.lock().unwrap().read(ChannelSelection::SingleA2)).unwrap()
    //                         );
    //                     }
    //                     let all_check = list.iter().all(|&x|x > 500);
    //                     if all_check {
    //                         *senser_mem.lock().unwrap()=true;
    //                         let mut state = report_mem.lock().unwrap().sensor_state;
    //                         state |= 0b0000_0010;
    //                         report_mem.lock().unwrap().sensor_state = state;
    //                         report_mem.lock().unwrap().save("Report");
    //                         list.clear();
    //                         continue;
    //                     }
    //                     else {
    //                         list.clear();
    //                         continue;
    //                     }
    //                 }
    //             }
    //             thread::sleep(Duration::from_millis(1));
    //         }
    //     });
    //     // #[cfg(unix)]
        
    // });
    // //=============모터부하 센서 측정=============
    // let arc_mem = arc_adc.clone();
    // let report_mem = report.clone();
    // thread::spawn(move||{
    //     let rt  = Runtime::new().unwrap();
    //     rt.block_on(async {
    //         loop{
    //             let sensor = block!(arc_mem.lock().unwrap().read(ChannelSelection::SingleA0)).unwrap();
    //             if report_mem.lock().unwrap().overload!=sensor{
    //                 report_mem.lock().unwrap().overload = sensor;
    //                 report_mem.lock().unwrap().save("Report");
    //             }
    //             thread::sleep(Duration::from_millis(50));
    //         }
    //     });
        
    // });

    //=============센서인식, 모터동작 제어스레드===========1==
    let senser1_mem = senser1_state.clone();
    let senser2_mem = senser2_state.clone();
    let moto_run_mem = moto_run.clone();
    let moto_dir_mem = moto_dir.clone();
    let app_sign_mem = app_sign.clone();
    thread::spawn(move||{
        let rt  = Runtime::new().unwrap();
        rt.block_on(async {
            loop{
                if *senser1_mem.lock().unwrap() || *senser2_mem.lock().unwrap(){
                    // println!("TEST");
                    if let None=*app_sign_mem.lock().unwrap(){
                        if moto_run_mem.lock().unwrap().is_set_high(){
                            // println!("LOW");
                            moto_run_mem.lock().unwrap().set_low();
                        }
                        if moto_dir_mem.lock().unwrap().is_set_high(){
                            // println!("HI");
                            moto_dir_mem.lock().unwrap().set_low();    
                        }
                    }
                    
                }

                thread::sleep(Duration::from_millis(1));
            }
        });
        
    });


    let arc_mem = arc_adc.clone();
    let moto_run_mem = moto_run.clone();
    let moto_dir_mem = moto_dir.clone();
    loop
    {   
        // if let Ok(data)=r.try_recv(){
        //     match data{
        //         true =>{
        //             if !moto_run_mem.lock().unwrap().is_set_high(){
        //                 moto_run_mem.lock().unwrap().set_high();

        //             }
        //             if moto_dir_mem.lock().unwrap().is_set_high(){
        //                 moto_dir_mem.lock().unwrap().set_low();
        //             }
        //         }
        //         false =>{
        //             if !moto_dir_mem.lock().unwrap().is_set_high(){
        //                 moto_dir_mem.lock().unwrap().set_high();
        //             }
        //             if !moto_run_mem.lock().unwrap().is_set_high(){
        //                 moto_run_mem.lock().unwrap().set_high();
        //             }
        //         }
        //     }
            
            
            // if let None=*app_sign_mem.lock().unwrap(){
            //     if moto_run_mem.lock().unwrap().is_set_high(){
            //         println!("LOW");
            //         moto_run_mem.lock().unwrap().set_low();
            //     }
            //     if moto_dir_mem.lock().unwrap().is_set_high(){
            //         println!("HI");
            //         moto_dir_mem.lock().unwrap().set_low();    
            //     }
            // }
        // }
        thread::sleep(Duration::from_millis(50));
    }
    Ok(())
}