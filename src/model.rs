#[derive(Clone, Debug, Copy)]
pub struct Vertex {
    pub pos: [f32; 4],
    pub color: [f32; 4], // TODO remove
}

#[derive(Debug)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

#[derive(Debug)]
pub struct Model {
    pub meshes: Vec<Mesh>,
}

#[derive(Debug)]
pub struct Scene {
    pub models: Vec<Model>,
}
