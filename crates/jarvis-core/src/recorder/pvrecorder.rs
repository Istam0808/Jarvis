use once_cell::sync::OnceCell;
use pv_recorder::{PvRecorder, PvRecorderBuilder};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static RECORDER: OnceCell<PvRecorder> = OnceCell::new();
static IS_RECORDING: AtomicBool = AtomicBool::new(false);
/// Throttle log spam when read() fails in a tight loop (e.g. INVALID_STATE).
static LAST_READ_ERR_LOG_MS: AtomicU64 = AtomicU64::new(0);

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn init_microphone(device_index: i32, frame_length: u32) -> bool {
    if RECORDER.get().is_some() {
        return true; // already initialized
    }
    
    // initialize
    let pv_recorder = PvRecorderBuilder::new(frame_length as i32)
        .device_index(device_index)
        // .frame_length(frame_length as i32)
        .init();

    match pv_recorder {
        Ok(pv) => {
            // store
            let _ = RECORDER.set(pv);

            // success
            true
        }
        Err(msg) => {
            error!("Failed to initialize pvrecorder.\nError details: {:?}", msg);

            // fail
            false
        }
    }
}

pub fn read_microphone(frame_buffer: &mut [i16]) {
    // ensure microphone is initialized
    if RECORDER.get().is_some() {
        // read to frame buffer

        let frame = RECORDER.get().unwrap().read();

        match frame {
            Ok(f) => {
                frame_buffer.copy_from_slice(f.as_slice());
            }
            Err(msg) => {
                // Не оставляем старый кадр — иначе VAD/wake видят «левый» сигнал.
                frame_buffer.fill(0);
                let t = now_ms();
                let last = LAST_READ_ERR_LOG_MS.load(Ordering::Relaxed);
                if t.saturating_sub(last) >= 1000 {
                    LAST_READ_ERR_LOG_MS.store(t, Ordering::Relaxed);
                    error!(
                        "Failed to read audio frame ({:?}). \
                        INVALID_STATE обычно значит: запись остановлена или сбой устройства — перезапустите ассистент, проверьте микрофон и что другой процесс не захватил его эксклюзивно.",
                        msg
                    );
                }
            }
        }
    }
}

pub fn start_recording(device_index: i32, frame_length: u32) -> Result<(), ()> {
    // ensure microphone is initialized
    init_microphone(device_index, frame_length);

    // start recording
    match RECORDER.get().unwrap().start() {
        Ok(_) => {
            info!("START recording from microphone ...");

            // change recording state
            IS_RECORDING.store(true, Ordering::SeqCst);

            // success
            Ok(())
        }
        Err(msg) => {
            error!("Failed to START audio recording: {}", msg);

            // fail
            Err(())
        }
    }
}

pub fn stop_recording() -> Result<(), ()> {
    // ensure microphone is initialized & recording is in process
    if RECORDER.get().is_some() && IS_RECORDING.load(Ordering::SeqCst) {
        // stop recording
        match RECORDER.get().unwrap().stop() {
            Ok(_) => {
                info!("STOP recording from microphone ...");

                // change recording state
                IS_RECORDING.store(false, Ordering::SeqCst);

                // success
                return Ok(());
            }
            Err(msg) => {
                error!("Failed to STOP audio recording: {}", msg);

                // fail
                return Err(());
            }
        }
    }

    Ok(()) // if already stopped or not yet initialized
}

pub fn list_audio_devices() -> Vec<String> {
    let audio_devices = PvRecorderBuilder::default().get_available_devices();
    match audio_devices {
        Ok(audio_devices) => audio_devices,
        Err(err) => {
            error!("Failed to get audio devices: {}", err);
            Vec::new()
        },
    }
}

pub fn get_audio_device_name(idx: i32) -> String {
    if idx == -1 {
        return String::from("System Default");
    }

    let audio_devices = list_audio_devices();
    let mut first_device: String = String::new();

    for (_idx, device) in audio_devices.iter().enumerate() {
        if idx as usize == _idx {
            return device.to_string();
        }

        if _idx == 0 {
            first_device = device.to_string()
        }
    }

    // return first device as default, if none were matched
    first_device
}
