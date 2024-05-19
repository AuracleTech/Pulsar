use super::Destroy;
use ash::{ext::debug_utils, vk, Entry};
use log::debug;
use std::{borrow::Cow, error::Error, ffi::CStr};

pub struct DebugUtils {
    debug_utils_loader: ash::ext::debug_utils::Instance,
    debug_call_back: vk::DebugUtilsMessengerEXT,
}

impl DebugUtils {
    pub fn new(entry: &Entry, instance: &ash::Instance) -> Result<Self, Box<dyn Error>> {
        unsafe {
            let debug_info = vk::DebugUtilsMessengerCreateInfoEXT::default()
                .message_severity(
                    vk::DebugUtilsMessageSeverityFlagsEXT::ERROR
                        | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                        | vk::DebugUtilsMessageSeverityFlagsEXT::INFO,
                )
                .message_type(
                    vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                        | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                        | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
                )
                .pfn_user_callback(Some(vulkan_debug_callback));
            let debug_utils_loader = debug_utils::Instance::new(entry, instance);
            let debug_call_back = debug_utils_loader
                .create_debug_utils_messenger(&debug_info, None)
                .unwrap();

            Ok(DebugUtils {
                debug_utils_loader,
                debug_call_back,
            })
        }
    }
}

pub extern "system" fn vulkan_debug_callback(
    message_severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    message_type: vk::DebugUtilsMessageTypeFlagsEXT,
    p_callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT<'_>,
    _user_data: *mut std::os::raw::c_void,
) -> vk::Bool32 {
    unsafe {
        let callback_data = *p_callback_data;
        let message_id_number = callback_data.message_id_number;

        let message_id_name = if callback_data.p_message_id_name.is_null() {
            Cow::from("")
        } else {
            CStr::from_ptr(callback_data.p_message_id_name).to_string_lossy()
        };

        let message = if callback_data.p_message.is_null() {
            Cow::from("")
        } else {
            CStr::from_ptr(callback_data.p_message).to_string_lossy()
        };

        debug!(
        "{message_severity:?}: {message_type:?} [{message_id_name} ({message_id_number})] : {message}",
    );

        vk::FALSE
    }
}

impl Destroy for DebugUtils {
    fn destroy(&mut self) {
        unsafe {
            self.debug_utils_loader
                .destroy_debug_utils_messenger(self.debug_call_back, None);
        }
    }
}
