use super::{device::AAADevice, surface_resources::AAAResources, AAABase};
use ash::{khr::surface, util::Align, vk};
use glam::Mat4;
use rwh_06::{HasDisplayHandle, HasWindowHandle};
use std::{error::Error, mem, sync::Arc};

pub struct AAASurface {
    pub surface_khr: vk::SurfaceKHR,
    pub format: vk::SurfaceFormatKHR,
    pub capabilities: vk::SurfaceCapabilitiesKHR,
    pub physical_device: vk::PhysicalDevice,
    pub queue_family_index: u32,
    pub resources: Option<AAAResources>,
}

impl AAASurface {
    pub fn new(
        renderer: &Arc<AAABase>,
        window: &winit::window::Window,
        physical_device_list: &[ash::vk::PhysicalDevice],
    ) -> Result<Self, Box<dyn Error>> {
        let surface_khr = unsafe {
            ash_window::create_surface(
                &renderer.entry,
                &renderer.instance,
                window.display_handle()?.as_raw(),
                window.window_handle()?.as_raw(),
                None,
            )
            .unwrap()
        };

        let (physical_device, queue_family_index) = physical_device_list
            .iter()
            .find_map(|physical_device| unsafe {
                renderer
                    .instance
                    .get_physical_device_queue_family_properties(*physical_device)
                    .iter()
                    .enumerate()
                    .find_map(|(index, info)| {
                        let supports_graphic_and_surface =
                            info.queue_flags.contains(vk::QueueFlags::GRAPHICS)
                                && renderer
                                    .surface_loader
                                    .get_physical_device_surface_support(
                                        *physical_device,
                                        index as u32,
                                        surface_khr,
                                    )
                                    .unwrap();
                        if supports_graphic_and_surface {
                            Some((*physical_device, index))
                        } else {
                            None
                        }
                    })
            })
            .expect("Couldn't find suitable device.");
        let queue_family_index = queue_family_index as u32;

        let format = unsafe {
            renderer
                .surface_loader
                .get_physical_device_surface_formats(physical_device, surface_khr)
                .unwrap()[0]
        };

        let capabilities = unsafe {
            renderer
                .surface_loader
                .get_physical_device_surface_capabilities(physical_device, surface_khr)
                .unwrap()
        };

        Ok(Self {
            surface_khr,
            format,
            capabilities,
            physical_device,
            queue_family_index,
            resources: None,
        })
    }

    pub fn recreate(&mut self, surface_loader: &surface::Instance) {
        self.format = unsafe {
            surface_loader
                .get_physical_device_surface_formats(self.physical_device, self.surface_khr)
                .unwrap()[0]
        };

        self.capabilities = unsafe {
            surface_loader
                .get_physical_device_surface_capabilities(self.physical_device, self.surface_khr)
                .unwrap()
        };
    }

    // pub fn update(&self, uniform: Mat4) {
    //     self.uniform *= Mat4::from_euler(glam::EulerRot::XYZ, 0.0, 0.0, 5); // TODO reinplement delta time
    //     self.update_uniform_buffer(&self.device, self.uniform_buffer_memory, self.uniform);
    // }

    fn update_uniform_buffer(
        device: &AAADevice,
        uniform_buffer_memory: vk::DeviceMemory,
        new_transform: Mat4,
    ) {
        unsafe {
            let uniform_ptr = device
                .ash
                .map_memory(
                    uniform_buffer_memory,
                    0,
                    mem::size_of::<Mat4>() as u64,
                    vk::MemoryMapFlags::empty(),
                )
                .unwrap();

            let mut uniform_aligned_slice = Align::new(
                uniform_ptr,
                mem::align_of::<Mat4>() as u64,
                mem::size_of::<Mat4>() as u64,
            );

            uniform_aligned_slice.copy_from_slice(&[new_transform]);
            device.ash.unmap_memory(uniform_buffer_memory);
        }
    }
}
