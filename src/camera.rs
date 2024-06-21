use glam::{Mat4, Vec3};

pub struct PerspectiveProjection {
    pub fov_y: f32,
    pub aspect_ratio: f32,
    pub near: f32,
    pub far: f32,
    pub view: Mat4,
    pub projection: Mat4,
    pub projection_view: Mat4,
}

impl PerspectiveProjection {
    pub fn new(fov_y: f32, aspect_ratio: f32, near: f32, far: f32, view: Mat4) -> Self {
        let projection = Mat4::perspective_rh(fov_y, aspect_ratio, near, far);
        Self {
            fov_y,
            aspect_ratio,
            near,
            far,
            view,
            projection,
            projection_view: projection * view,
        }
    }

    pub fn update(&mut self) {
        self.projection = Mat4::perspective_rh(self.fov_y, self.aspect_ratio, self.near, self.far);
        self.projection_view = self.projection * self.view;
    }
}

pub struct OrthographicProjection {
    pub left: f32,
    pub right: f32,
    pub bottom: f32,
    pub top: f32,
    pub near: f32,
    pub far: f32,
    pub view: Mat4,
    pub projection: Mat4,
    pub projection_view: Mat4,
}

impl OrthographicProjection {
    pub fn new(
        left: f32,
        right: f32,
        bottom: f32,
        top: f32,
        near: f32,
        far: f32,
        view: Mat4,
    ) -> Self {
        let projection = Mat4::orthographic_rh(left, right, bottom, top, near, far);
        Self {
            left,
            right,
            bottom,
            top,
            near,
            far,
            view,
            projection,
            projection_view: projection * view,
        }
    }

    pub fn update(&mut self) {
        self.projection = Mat4::orthographic_rh(
            self.left,
            self.right,
            self.bottom,
            self.top,
            self.near,
            self.far,
        );
        self.projection_view = self.projection * self.view;
    }
}

pub struct Camera {
    pub position: Vec3,
    pub view: Mat4,
    pub orthographic: OrthographicProjection,
    pub perspective: PerspectiveProjection,
}

impl Camera {
    pub fn new(
        position: Vec3,
        orthographic_projections: OrthographicProjection,
        perspective_projections: PerspectiveProjection,
    ) -> Self {
        let view = Mat4::look_at_rh(position, Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0));
        Self {
            position,
            view,
            orthographic: orthographic_projections,
            perspective: perspective_projections,
        }
    }

    pub fn update(&mut self) {
        self.orthographic.update();
        self.perspective.update();
        self.view = Mat4::look_at_rh(
            self.position,
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        );
    }
}
