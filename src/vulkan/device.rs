use ash::{khr::swapchain, vk};

pub struct AAADevice {
    pub ash: ash::Device,
}

impl AAADevice {
    pub fn new(
        instance: &ash::Instance,
        pdevice: vk::PhysicalDevice,
        queue_family_index: u32,
    ) -> Self {
        let priorities = [1.0];
        let queue_info = vk::DeviceQueueCreateInfo::default()
            .queue_family_index(queue_family_index)
            .queue_priorities(&priorities);
        let device_extension_names_raw = [
            swapchain::NAME.as_ptr(),
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            ash::khr::portability_subset::NAME.as_ptr(),
        ];
        let features = vk::PhysicalDeviceFeatures {
            shader_clip_distance: 1,
            ..Default::default()
        };
        let device_create_info = vk::DeviceCreateInfo::default()
            .queue_create_infos(std::slice::from_ref(&queue_info))
            .enabled_extension_names(&device_extension_names_raw)
            .enabled_features(&features);
        let ash = unsafe {
            instance
                .create_device(pdevice, &device_create_info, None)
                .unwrap()
        };

        Self { ash }
    }
}

impl Drop for AAADevice {
    fn drop(&mut self) {
        unsafe {
            self.ash.destroy_device(None);
        }
    }
}
