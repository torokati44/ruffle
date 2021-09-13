use ruffle_core::backend::audio::{
    self, swf, AudioBackend, SoundHandle, SoundInstanceHandle, SoundTransform,
};
use ruffle_core::impl_audio_mixer_backend;
use ruffle_web_common::JsResult;
use wasm_bindgen::{closure::Closure, prelude::*, JsCast};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    AudioContext, AudioProcessingEvent, AudioWorkletNode, AudioWorkletNodeOptions,
    ScriptProcessorNode,
};

type Error = Box<dyn std::error::Error>;

#[allow(dead_code)]
pub struct WebAudioMixerBackend {
    mixer: audio::AudioMixer,
    context: AudioContext,
    audio_worklet: Option<AudioWorkletNode>,
    script_processor: Option<ScriptProcessorNode>,
    on_audio_process: Option<Closure<dyn FnMut(AudioProcessingEvent)>>,
}

impl WebAudioMixerBackend {
    pub async fn new() -> Result<Self, Error> {
        let context = AudioContext::new().map_err(|_| "Unable to create AudioContext")?;
        let mixer = audio::AudioMixer::new(2, context.sample_rate() as u32);

        let worklet = context.audio_worklet().into_js_result()?;
        let _ = JsFuture::from(build_audio_worklet(&worklet))
            .await
            .into_js_result()?;

        let options = [("sampleRate", context.sample_rate().into())];
        let processor_options = js_sys::Object::new();
        for (name, value) in options.iter() {
            js_sys::Reflect::set(&processor_options, &JsValue::from_str(name), value)
                .warn_on_error();
        }
        let mut worklet_options = AudioWorkletNodeOptions::new();
        worklet_options.processor_options(Some(&processor_options));

        let worklet_node = AudioWorkletNode::new_with_options(
            &context,
            "ruffle-audio-processor",
            &worklet_options,
        )
        .into_js_result()?;
        worklet_node
            .connect_with_audio_node(&context.destination())
            .warn_on_error();

        let port = worklet_node.port().into_js_result()?;
        //let _ = port.post_message(&wasm_bindgen::memory());
        /*let script_processor = context.create_script_processor_with_buffer_size_and_number_of_input_channels_and_number_of_output_channels(0, 0, 2).map_err(|_| "Unable to create ScriptProcessorNode")?;

        let mixer_proxy = mixer.proxy();
        let buffer_samples = 2 * script_processor.buffer_size() as usize;
        let mut out_data = Vec::new();
        out_data.resize(buffer_samples, 0.0);
        let on_audio_process = move |event: AudioProcessingEvent| {
            if let Ok(output_buffer) = event.output_buffer() {
                mixer_proxy.mix(&mut out_data);
                copy_to_audio_buffer_interleaved(&output_buffer, &out_data);
            }
        };
        let on_audio_process =
            Closure::wrap(Box::new(on_audio_process) as Box<dyn FnMut(AudioProcessingEvent)>);
        script_processor.set_onaudioprocess(Some(on_audio_process.as_ref().unchecked_ref()));
        script_processor
            .connect_with_audio_node(&context.destination())
            .warn_on_error();
            */

        Ok(Self {
            mixer,
            context,
            audio_worklet: Some(worklet_node),
            script_processor: None,
            on_audio_process: None,
        })
    }

    /// Returns the JavaScript AudioContext.
    pub fn audio_context(&self) -> &AudioContext {
        &self.context
    }
}

impl AudioBackend for WebAudioMixerBackend {
    impl_audio_mixer_backend!(mixer);

    fn play(&mut self) {
        let _ = self.context.resume();
    }

    fn pause(&mut self) {
        let _ = self.context.suspend();
    }
}

impl Drop for WebAudioMixerBackend {
    fn drop(&mut self) {
        if let Some(script_processor) = &self.script_processor {
            script_processor.set_onaudioprocess(None);
        }
        let _ = self.context.close();
    }
}

#[wasm_bindgen(raw_module = "./ruffle-imports.js")]
extern "C" {
    /// Imported JS method to copy interleaved audio data into an `AudioBuffer`.
    #[wasm_bindgen(js_name = "copyToAudioBufferInterleaved")]
    fn copy_to_audio_buffer_interleaved(
        audio_buffer: &web_sys::AudioBuffer,
        interleaved_data: &[f32],
    );

    /// Imported JS method to copy interleaved audio data into an `AudioBuffer`.
    #[wasm_bindgen(js_name = "buildAudioWorklet")]
    fn build_audio_worklet(worklet: &web_sys::AudioWorklet) -> js_sys::Promise;
}

#[wasm_bindgen]
pub struct AudioMixer {
    mixer: audio::AudioMixer,
}

#[wasm_bindgen]
impl AudioMixer {
    #[wasm_bindgen(constructor)]
    pub fn new(sample_rate: u32) -> Self {
        Self {
            mixer: audio::AudioMixer::new(2, sample_rate),
        }
    }

    // pub fn register_sound(&mut self, swf_sound: &swf::Sound) {
    //     let _ = self.mixer.register_sound(swf_sound);
    // }

    // // #[inline]
    // // pub fn start_stream(
    // //     &mut self,
    // //     stream_handle: Option<SoundHandle>,
    // //     clip_frame: u16,
    // //     clip_data: $crate::tag_utils::SwfSlice,
    // //     stream_info: &swf::SoundStreamHead,
    // // ) -> Result<SoundInstanceHandle, Error> {
    // //     self.mixer
    // //         .start_stream(stream_handle, clip_frame, clip_data, stream_info)
    // // }

    // #[inline]
    // pub fn start_sound(
    //     &mut self,
    //     sound_handle: SoundHandle,
    //     settings: &swf::SoundInfo,
    // ) -> Result<SoundInstanceHandle, Error> {
    //     self.mixer.start_sound(sound_handle, settings)
    // }

    // #[inline]
    // pub fn stop_sound(&mut self, sound: SoundInstanceHandle) {
    //     self.mixer.stop_sound(sound)
    // }

    // #[inline]
    // pub fn stop_all_sounds(&mut self) {
    //     self.mixer.stop_all_sounds()
    // }

    // // #[inline]
    // // pub fn get_sound_format(&self, sound: SoundHandle) -> Option<&swf::SoundFormat> {
    // //     self.mixer.get_sound_format(sound)
    // // }

    // #[inline]
    // pub fn set_sound_transform(
    //     &mut self,
    //     instance: SoundInstanceHandle,
    //     transform: SoundTransform,
    // ) {
    //     self.mixer.set_sound_transform(instance, transform)
    // }
}

#[wasm_bindgen]
struct AudioMixerProxy(ruffle_core::backend::audio::AudioMixerProxy);

impl AudioMixerProxy {
    // /// Mixes audio into the given `output_buffer`.
    // ///
    // /// All playing sound instances will be sampled and mixed to fill `output_buffer`.
    // /// `output_buffer` is expected to be in 2-channel interleaved format.
    // pub fn mix<'a, T>(&self, output_buffer: &mut [T])
    // where
    //     T: 'a + dasp::Sample + Default,
    //     T::Signed: dasp::sample::conv::FromSample<i16>,
    //     T::Float: dasp::sample::conv::FromSample<f32>,
    // {
    //     let mut sound_instances = self.sound_instances.lock().unwrap();
    //     AudioMixer::mix_audio::<T>(
    //         &mut sound_instances,
    //         self.num_output_channels,
    //         output_buffer,
    //     )
    // }
}

// fn mix_worklet(proxy: &mut AudioMixerProxy) {
//     if let Ok(output_buffer) = event.output_buffer() {
//         mixer_proxy.mix(&mut out_data);
//         copy_to_audio_buffer_interleaved(&output_buffer, &out_data);
//     }
// }
