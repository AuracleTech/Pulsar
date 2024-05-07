use ash::{util::*, vk};
use std::{ffi, io::Cursor, path::Path};

const COMPILE_SHADERS_PATH: &str = "assets/bin/";

pub struct Shader<'a> {
    pub module: vk::ShaderModule,
    pub pipeline_shader_stage_create_info: vk::PipelineShaderStageCreateInfo<'a>,
}

impl<'a> Shader<'a> {
    pub fn from_filename(
        filename: &str,
        stage: vk::ShaderStageFlags,
        device: &ash::Device,
    ) -> Shader<'a> {
        let path = format!("assets/bin/{}.spv", filename);
        if !Path::new(&path).exists() {
            panic!("Shader not compiled: {}", path);
        }
        let file_content = std::fs::read(path).expect("Failed to read shader file");
        let bytecode = Vec::<u8>::from(file_content);
        let mut shader_bin_cursor = Cursor::new(bytecode);

        let shader_aligned =
            read_spv(&mut shader_bin_cursor).expect("Failed to read vertex shader spv file");
        let shader_info = vk::ShaderModuleCreateInfo::default().code(&shader_aligned);

        unsafe {
            let shader_module = device
                .create_shader_module(&shader_info, None)
                .expect("Vertex shader module error");

            let shader_entry_name = ffi::CStr::from_bytes_with_nul_unchecked(b"main\0");

            let mut pipeline_shader_stage_create_info = vk::PipelineShaderStageCreateInfo {
                module: shader_module,
                p_name: shader_entry_name.as_ptr(),
                stage,
                ..Default::default()
            };

            if stage == vk::ShaderStageFlags::FRAGMENT {
                pipeline_shader_stage_create_info.s_type =
                    vk::StructureType::PIPELINE_SHADER_STAGE_CREATE_INFO;
            }

            Self {
                module: shader_module,
                pipeline_shader_stage_create_info,
            }
        }
    }

    pub fn compile_shaders() {
        if Path::new(COMPILE_SHADERS_PATH).exists() {
            let files =
                std::fs::read_dir(COMPILE_SHADERS_PATH).expect("Failed to read shader files");
            for file in files {
                let file = file.expect("Failed to read shader files");
                let path = file.path();
                if let Some(ext) = path.extension() {
                    if ext == "spv" {
                        std::fs::remove_file(path).expect("Failed to remove old shader files");
                    }
                }
            }
        } else {
            std::fs::create_dir(COMPILE_SHADERS_PATH).expect("Failed to create shader directory");
        }

        let output_vert = std::process::Command::new("glslc.exe")
            .arg("assets/shaders/shader.vert")
            .arg("-o")
            .arg(format!("{}/vert.spv", COMPILE_SHADERS_PATH))
            .output()
            .expect("Failed to execute glslc.exe for vertex shader");

        let output_frag = std::process::Command::new("glslc.exe")
            .arg("assets/shaders/shader.frag")
            .arg("-o")
            .arg(format!("{}/frag.spv", COMPILE_SHADERS_PATH))
            .output()
            .expect("Failed to execute glslc.exe for fragment shader");

        if !(output_vert.status.success() && output_frag.status.success()) {
            panic!(
                "Failed to compile shaders:\n{}\n{}",
                String::from_utf8_lossy(&output_vert.stderr),
                String::from_utf8_lossy(&output_frag.stderr)
            );
        }
    }
}
