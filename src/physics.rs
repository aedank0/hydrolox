use std::sync::{mpsc::Sender, RwLock};

use hydrolox_pga3d::prelude as pga;
use serde::{Deserialize, Serialize};

use crate::{
    framework::{Component, Components, Comptainer, Entity},
    game::{Process, ProcessStage, Transform, STEP_PERIOD},
    geometry::{Vec3, AABB},
    input::Input,
    render::RenderMessage,
};

fn gjk_support_verts<I: IntoIterator<Item = Vec3>>(dir: Vec3, verts: I) -> Vec3
where
    I::IntoIter: Clone,
{
    verts
        .into_iter()
        .max_by(|a, b| a.dot(dir).partial_cmp(&b.dot(dir)).expect("NaN"))
        .unwrap()
}

fn box_verts(dimensions: &Vec3) -> [Vec3; 8] {
    [
        Vec3::new(
            -dimensions.x * 0.5,
            -dimensions.y * 0.5,
            -dimensions.z * 0.5,
        ),
        Vec3::new(-dimensions.x * 0.5, dimensions.y * 0.5, -dimensions.z * 0.5),
        Vec3::new(-dimensions.x * 0.5, dimensions.y * 0.5, dimensions.z * 0.5),
        Vec3::new(-dimensions.x * 0.5, -dimensions.y * 0.5, dimensions.z * 0.5),
        Vec3::new(dimensions.x * 0.5, -dimensions.y * 0.5, -dimensions.z * 0.5),
        Vec3::new(dimensions.x * 0.5, dimensions.y * 0.5, -dimensions.z * 0.5),
        Vec3::new(dimensions.x * 0.5, dimensions.y * 0.5, dimensions.z * 0.5),
        Vec3::new(dimensions.x * 0.5, -dimensions.y * 0.5, dimensions.z * 0.5),
    ]
}

fn box_verts_transformed(dimensions: &Vec3, motor: &pga::Motor) -> [Vec3; 8] {
    [
        motor
            .transform(pga::Point::from_position(
                -dimensions.x * 0.5,
                -dimensions.y * 0.5,
                -dimensions.z * 0.5,
            ))
            .into(),
        motor
            .transform(pga::Point::from_position(
                -dimensions.x * 0.5,
                dimensions.y * 0.5,
                -dimensions.z * 0.5,
            ))
            .into(),
        motor
            .transform(pga::Point::from_position(
                -dimensions.x * 0.5,
                dimensions.y * 0.5,
                dimensions.z * 0.5,
            ))
            .into(),
        motor
            .transform(pga::Point::from_position(
                -dimensions.x * 0.5,
                -dimensions.y * 0.5,
                dimensions.z * 0.5,
            ))
            .into(),
        motor
            .transform(pga::Point::from_position(
                dimensions.x * 0.5,
                -dimensions.y * 0.5,
                -dimensions.z * 0.5,
            ))
            .into(),
        motor
            .transform(pga::Point::from_position(
                dimensions.x * 0.5,
                dimensions.y * 0.5,
                -dimensions.z * 0.5,
            ))
            .into(),
        motor
            .transform(pga::Point::from_position(
                dimensions.x * 0.5,
                dimensions.y * 0.5,
                dimensions.z * 0.5,
            ))
            .into(),
        motor
            .transform(pga::Point::from_position(
                dimensions.x * 0.5,
                -dimensions.y * 0.5,
                dimensions.z * 0.5,
            ))
            .into(),
    ]
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ColliderShape {
    Box { dimensions: Vec3 },
    Sphere { radius: f32 },
}
impl Component for ColliderShape {}
impl ColliderShape {
    fn gjk_support(&self, dir: Vec3, motor: &pga::Motor) -> Vec3 {
        match self {
            Self::Box { dimensions } => {
                gjk_support_verts(dir, box_verts_transformed(dimensions, motor))
            }
            Self::Sphere { radius } => {
                dir.normalized() * *radius + Vec3::from(motor.translation_euler())
            }
        }
    }
    fn aabb(&self, motor: &pga::Motor) -> AABB {
        match self {
            Self::Box { dimensions } => AABB::from_verts(box_verts_transformed(dimensions, motor)),
            Self::Sphere { radius } => AABB::new(
                Vec3::new(-*radius, -*radius, -*radius),
                Vec3::new(*radius, *radius, *radius),
            ),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CollisionEvent {}

#[derive(Debug, Serialize, Deserialize)]
pub enum CollisionType {
    Discrete,
    Continuous,
}
impl CollisionType {
    fn is_continuous(&self) -> bool {
        match self {
            Self::Continuous => true,
            _ => false,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Collider {
    pub shapes: Vec<Entity>,
    pub events: Vec<CollisionEvent>,
    pub collision_type: CollisionType,
}
impl Collider {
    fn aabb(
        &self,
        transforms: &Comptainer<Transform>,
        col_shapes: &Comptainer<ColliderShape>,
    ) -> AABB {
        let mut min = Vec3::MAX;
        let mut max = Vec3::MIN;

        for (motor, col_shape) in self.shapes.iter().map(|&e| {
            let transform = transforms.get(e).unwrap();
            (
                transform.global_motor(transforms),
                col_shapes.get(e).unwrap(),
            )
        }) {
            let aabb = col_shape.aabb(&motor);
            min = aabb.min.min_components(min);
            max = aabb.max.max_components(max);
        }

        AABB::new(min, max)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PhysicsBody {
    pub mass: f32,
    pub angular_inertia: f32,
    pub linear_velocity: Vec3,
    pub angular_velocity: Vec3,
    linear_imp: Vec3,
    angular_imp: Vec3,
    pub collision: Option<Collider>,
}
impl PhysicsBody {
    pub fn apply_force(&mut self, force: Vec3, location: Vec3) {
        self.apply_impulse(force * STEP_PERIOD, location);
    }
    pub fn apply_force_central(&mut self, force: Vec3) {
        self.apply_impulse_central(force * STEP_PERIOD);
    }
    pub fn apply_impulse(&mut self, impulse: Vec3, location: Vec3) {
        self.angular_imp += location.cross(impulse);

        let loc_norm = location.normalized();
        self.linear_imp += loc_norm * loc_norm.dot(impulse);
    }
    pub fn apply_impulse_central(&mut self, impulse: Vec3) {
        self.linear_imp += impulse;
    }
}
impl Default for PhysicsBody {
    fn default() -> Self {
        Self {
            mass: 1.0,
            angular_inertia: 0.5,
            linear_velocity: Vec3::ZERO,
            angular_velocity: Vec3::ZERO,
            linear_imp: Vec3::ZERO,
            angular_imp: Vec3::ZERO,
            collision: None,
        }
    }
}
impl Component for PhysicsBody {}

#[derive(Debug)]
pub struct PhysicsProcess {}
impl Process for PhysicsProcess {
    fn stage(&self) -> ProcessStage {
        ProcessStage::Physics
    }

    fn new(_: &Components, _: &Sender<RenderMessage>) -> Self
    where
        Self: Sized,
    {
        Self {}
    }
    fn step(&mut self, components: &Components, _: &RwLock<Input>) {
        let mut transforms = components.transforms.write().unwrap();
        let mut physics_bodies = components.physics_bodies.write().unwrap();
        let collider_shapes = components.collision_shapes.read().unwrap();

        let mut octree = Octree::new();

        for (e, physics_body) in physics_bodies.iter_mut() {
            let transform = transforms.get_mut(e).unwrap();

            physics_body.linear_velocity += physics_body.linear_imp / physics_body.mass;
            let delta_pos = physics_body.linear_velocity * STEP_PERIOD;
            physics_body.linear_imp = Vec3::ZERO;

            physics_body.angular_velocity +=
                physics_body.angular_imp / physics_body.angular_inertia;
            let delta_rot = physics_body.angular_velocity * STEP_PERIOD;
            physics_body.angular_imp = Vec3::ZERO;

            let prev_motor = transform.motor;
            let next_motor = pga::Motor::from_euler_angles(delta_rot.x, delta_rot.y, delta_rot.z)
                .combine(prev_motor.combine(pga::Motor::from_translation(
                    delta_pos.x,
                    delta_pos.y,
                    delta_pos.z,
                )));

            if let Some(collider) = &physics_body.collision {
                if !collider.shapes.is_empty() {
                    let parent_motor = transform.clone().parent_motor(&transforms);
                    let prev_global = parent_motor.map_or(prev_motor, |m| m.combine(prev_motor));
                    let next_global = parent_motor.map_or(next_motor, |m| m.combine(next_motor));
                    let mut min = Vec3::MAX;
                    let mut max = Vec3::MIN;

                    for (shape_motor, col_shape) in collider.shapes.iter().map(|&e| {
                        let col_transform = transforms.get(e).unwrap();
                        (col_transform.motor, collider_shapes.get(e).unwrap())
                    }) {
                        let aabb = col_shape.aabb(&next_global.combine(shape_motor));
                        min = aabb.min.min_components(min);
                        max = aabb.max.max_components(max);
                        if collider.collision_type.is_continuous() {
                            let aabb = col_shape.aabb(&prev_global.combine(shape_motor));
                            min = aabb.min.min_components(min);
                            max = aabb.max.max_components(max);
                        }
                    }

                    let aabb = AABB::new(min, max);

                    octree.insert(PhysData::new(aabb, e, next_motor, delta_pos, delta_rot));

                    continue;
                }
            }

            transform.motor = next_motor;
        }

        octree.traverse(|a, b| {
            todo!("Detect and resolve collision for pairs");
        });
    }
}

#[derive(Debug)]
struct PhysData {
    aabb: AABB,
    entity: Entity,
    next_motor: pga::Motor,
    delta_pos: Vec3,
    delta_rot: Vec3,
}
impl PhysData {
    fn new(
        aabb: AABB,
        entity: Entity,
        next_motor: pga::Motor,
        delta_pos: Vec3,
        delta_rot: Vec3,
    ) -> Self {
        Self {
            aabb,
            entity,
            next_motor,
            delta_pos,
            delta_rot,
        }
    }
}

#[derive(Debug, Default)]
struct OctNode {
    depth: u8,
    origin: Vec3,
    data: Vec<PhysData>,
    children: Option<[Option<Box<OctNode>>; 8]>,
}
impl OctNode {
    fn with_origin(origin: Vec3) -> Self {
        Self {
            depth: 0,
            origin,
            data: Vec::default(),
            children: None,
        }
    }

    fn insert(&mut self, phys_data: PhysData) {
        if self.depth >= 14
            || (self.data.is_empty() && self.children.is_none())
            || (phys_data.aabb.min.x <= self.origin.x && phys_data.aabb.max.x >= self.origin.x)
            || (phys_data.aabb.min.y <= self.origin.y && phys_data.aabb.max.y >= self.origin.y)
            || (phys_data.aabb.min.z <= self.origin.z && phys_data.aabb.max.z >= self.origin.z)
        {
            //at leaf node or aabb intersects origin
            self.data.push(phys_data);
        } else {
            if self.children.is_none() {
                self.children = Default::default();
                for pd in std::mem::take(&mut self.data) {
                    self.insert(pd);
                }
            }

            let mut index = 0usize;
            if phys_data.aabb.min.x >= self.origin.x {
                index |= 0b001;
            }
            if phys_data.aabb.min.y >= self.origin.y {
                index |= 0b010;
            }
            if phys_data.aabb.min.z >= self.origin.z {
                index |= 0b100;
            }

            self.children.as_mut().unwrap()[index]
                .get_or_insert_with(|| {
                    let mut child_origin = self.origin;
                    let offset = (8192u32 >> self.depth) as f32 * 0.5;

                    if (index & 0b001) != 0 {
                        child_origin.x += offset;
                    } else {
                        child_origin.x -= offset;
                    }
                    if (index & 0b010) != 0 {
                        child_origin.y += offset;
                    } else {
                        child_origin.y -= offset;
                    }
                    if (index & 0b100) != 0 {
                        child_origin.z += offset;
                    } else {
                        child_origin.z -= offset;
                    }

                    Box::new(OctNode::with_origin(child_origin))
                })
                .insert(phys_data);
        }
    }

    fn traverse<F: FnMut(&PhysData, &PhysData)>(&self, func: &mut F, parent: Option<&PhysData>) {
        let mut iter = parent.into_iter().chain(self.data.iter());
        while let Some(pd) = iter.next() {
            for pd2 in iter.clone() {
                func(pd, pd2);
            }

            if let Some(children) = &self.children {
                for child_maybe in children {
                    if let Some(child) = child_maybe {
                        child.traverse(func, Some(pd));
                    }
                }
            }
        }
    }
}

#[derive(Debug, Default)]
struct Octree {
    root: OctNode,
}
impl Octree {
    fn new() -> Self {
        Self::default()
    }

    fn insert(&mut self, phys_data: PhysData) {
        self.root.insert(phys_data);
    }

    fn traverse<F: FnMut(&PhysData, &PhysData)>(&self, mut func: F) {
        self.root.traverse(&mut func, None);
    }
}
