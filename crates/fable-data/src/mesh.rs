//! Meshes in the `MBANK_ALLMESHES` bank of `graphics/graphics.big`.
//! See `docs/formats/mesh.md`.
//!
//! Decoded: the info block, and each level of detail's materials, sub-meshes,
//! vertices (position, normal, texture coordinates) and indices, for static
//! meshes (type 1) and skinned meshes (type 5, characters and creatures), whose
//! levels also carry a skeleton and per-vertex bone weights. Type 4 is not
//! supported yet.

use crate::reader::{Reader, latin1};
use crate::{Error, Result, lzo};

/// How a level's chunks are laid out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Layout {
    /// Static meshes: sub-mesh records name their material.
    Static,
    /// Skinned meshes: the chunk names the material; records carry bone lists and
    /// vertices bone weights.
    Skinned,
    /// Static meshes laid out like skinned ones (each level starts with the name
    /// and a skeleton) but with static chunks.
    Rigged,
}

/// Bank entry type of static meshes.
pub const STATIC_MESH_TYPE: u32 = 1;
/// Bank entry type of skinned meshes (with a skeleton).
pub const SKINNED_MESH_TYPE: u32 = 5;

/// The bank entry's info block.
#[derive(Debug, Clone, PartialEq)]
pub struct MeshInfo {
    pub sphere_center: [f32; 3],
    pub sphere_radius: f32,
    pub box_min: [f32; 3],
    pub box_max: [f32; 3],
    /// Byte length of each level of detail, most detailed first.
    pub lod_sizes: Vec<u32>,
    /// One value per level after the first; possibly switch distances.
    pub lod_switches: Vec<f32>,
    /// Texture bank entry ids used by the mesh.
    pub texture_ids: Vec<u32>,
}

impl MeshInfo {
    pub fn parse(info: &[u8]) -> Result<Self> {
        let mut r = Reader::new(info);
        let _unknown = r.u32()?;
        let mut f = || r.u32().map(f32::from_bits);
        let sphere_center = [f()?, f()?, f()?];
        let sphere_radius = f()?;
        let box_min = [f()?, f()?, f()?];
        let box_max = [f()?, f()?, f()?];
        let lods = r.u32()? as usize;
        if lods == 0 || lods > 16 {
            return Err(Error::Invalid {
                offset: r.pos() - 4,
                reason: format!("{lods} levels of detail"),
            });
        }
        let lod_sizes = (0..lods).map(|_| r.u32()).collect::<Result<_>>()?;
        let _unknown = r.u32()?;
        let lod_switches = (1..lods)
            .map(|_| r.u32().map(f32::from_bits))
            .collect::<Result<_>>()?;
        let textures = r.u32()? as usize;
        let texture_ids = (0..textures).map(|_| r.u32()).collect::<Result<_>>()?;
        Ok(Self {
            sphere_center,
            sphere_radius,
            box_min,
            box_max,
            lod_sizes,
            lod_switches,
            texture_ids,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Mesh {
    pub info: MeshInfo,
    pub lods: Vec<Lod>,
}

impl Mesh {
    /// Parses an entry's data. The levels of detail are the last bytes of the
    /// entry, back to back; the header before them is not decoded yet.
    pub fn parse(info: &[u8], data: &[u8]) -> Result<Self> {
        let info = MeshInfo::parse(info)?;
        let total: usize = info.lod_sizes.iter().map(|&s| s as usize).sum();
        let mut at = data.len().checked_sub(total).ok_or(Error::Invalid {
            offset: 0,
            reason: format!("levels of detail ({total} bytes) larger than the entry"),
        })?;
        // The entry starts with the mesh name; each level of detail ends with a
        // trailer that starts with it again.
        let name = &data[..data.iter().position(|&b| b == 0).unwrap_or(0)];
        let mut lods = Vec::with_capacity(info.lod_sizes.len());
        for &size in &info.lod_sizes {
            lods.push(Lod::parse(&data[at..at + size as usize], name).map_err(|e| shift(e, at))?);
            at += size as usize;
        }
        Ok(Self { info, lods })
    }

    /// Parses a skinned mesh entry. Each level of detail is self-contained (it
    /// repeats the name and the skeleton) and they start at the beginning of the
    /// entry, in the order and sizes of the info block.
    pub fn parse_skinned(info: &[u8], data: &[u8]) -> Result<Self> {
        Self::parse_with_skeleton(info, data, Layout::Skinned)
    }

    /// Parses a static (type 1) entry, falling back to the rigged layout: some
    /// static meshes (the charred villagers, statues) are laid out like skinned
    /// meshes, each level with a skeleton, but with static chunks.
    pub fn parse_static(info: &[u8], data: &[u8]) -> Result<Self> {
        Self::parse(info, data)
            .or_else(|e| Self::parse_with_skeleton(info, data, Layout::Rigged).map_err(|_| e))
    }

    fn parse_with_skeleton(info: &[u8], data: &[u8], layout: Layout) -> Result<Self> {
        let info = MeshInfo::parse(info)?;
        let mut at = 0usize;
        let mut lods = Vec::with_capacity(info.lod_sizes.len());
        for &size in &info.lod_sizes {
            let block = data
                .get(at..at + size as usize)
                .ok_or(Error::UnexpectedEof {
                    offset: at,
                    wanted: size as usize,
                })?;
            lods.push(Lod::parse_skinned(block, layout).map_err(|e| shift(e, at))?);
            at += size as usize;
        }
        Ok(Self { info, lods })
    }
}

fn shift(e: Error, by: usize) -> Error {
    match e {
        Error::Invalid { offset, reason } => Error::Invalid {
            offset: offset + by,
            reason,
        },
        Error::UnexpectedEof { offset, wanted } => Error::UnexpectedEof {
            offset: offset + by,
            wanted,
        },
        e => e,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Material {
    pub id: u32,
    pub name: String,
    /// Texture bank entry id of the main texture, 0 if none.
    pub texture: u32,
    /// A second texture id (0 if none); its role is not known yet.
    pub second_texture: u32,
    /// The 33 bytes after the name.
    pub raw: [u8; 33],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubMesh {
    /// Triangles in this sub-mesh.
    pub primitive_count: u32,
    pub first_index: u32,
    /// Indices form a triangle strip rather than a list.
    pub strip: bool,
    /// Index into the level's materials.
    pub material: u32,
    pub raw_flags: [u8; 3],
    /// Skinned meshes: the skeleton bones this sub-mesh's vertices refer to; a
    /// vertex's bone index `i` means `bones[i]`. Empty for static meshes.
    pub bones: Vec<u8>,
}

impl SubMesh {
    pub fn index_count(&self) -> u32 {
        if self.strip {
            self.primitive_count + 2
        } else {
            self.primitive_count * 3
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Lod {
    /// Skinned meshes only.
    pub skeleton: Option<Skeleton>,
    pub materials: Vec<Material>,
    /// Pieces of geometry, each with its own vertices (the Oakvale tavern has 4).
    pub chunks: Vec<Chunk>,
}

/// The bones of a skinned mesh, parents before children.
#[derive(Debug, Clone, PartialEq)]
pub struct Skeleton {
    /// Attachment points and dummies (`focus_camera_01`, `WEAPON`, …); not decoded.
    pub helper_count: u16,
    pub dummy_count: u16,
    pub bones: Vec<Bone>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Bone {
    pub name: String,
    pub parent: Option<usize>,
    /// Model space to bone space in the bind pose: 3 rows of a 4x4 matrix whose
    /// last row is (0, 0, 0, 1), acting on column vectors.
    pub inverse_bind: [[f32; 4]; 3],
    /// Rest pose relative to the parent, as stored: `rotation` is a quaternion
    /// (x, y, z, w) of the inverse rotation (see [`Bone::local`]).
    pub rotation: [f32; 4],
    pub translation: [f32; 3],
    pub scale: [f32; 3],
}

/// A 3x4 affine matrix (rows), acting on column vectors.
pub type Affine = [[f32; 4]; 3];

impl Bone {
    /// The rest pose relative to the parent: translate, rotate by the conjugate of
    /// [`Bone::rotation`], scale. Composing these from the root reproduces the
    /// inverse of every [`Bone::inverse_bind`] (checked on the young Hero).
    pub fn local(&self) -> Affine {
        let [x, y, z, w] = self.rotation;
        let (x, y, z) = (-x, -y, -z);
        let r = [
            [
                1.0 - 2.0 * (y * y + z * z),
                2.0 * (x * y - z * w),
                2.0 * (x * z + y * w),
            ],
            [
                2.0 * (x * y + z * w),
                1.0 - 2.0 * (x * x + z * z),
                2.0 * (y * z - x * w),
            ],
            [
                2.0 * (x * z - y * w),
                2.0 * (y * z + x * w),
                1.0 - 2.0 * (x * x + y * y),
            ],
        ];
        std::array::from_fn(|i| {
            [
                r[i][0] * self.scale[0],
                r[i][1] * self.scale[1],
                r[i][2] * self.scale[2],
                self.translation[i],
            ]
        })
    }
}

/// `a * b` for affine matrices.
pub fn mul_affine(a: &Affine, b: &Affine) -> Affine {
    std::array::from_fn(|i| {
        std::array::from_fn(|j| {
            let t = if j == 3 { a[i][3] } else { 0.0 };
            t + (0..3).map(|k| a[i][k] * b[k][j]).sum::<f32>()
        })
    })
}

impl Skeleton {
    /// Each bone's rest pose in model space.
    pub fn rest_pose(&self) -> Vec<Affine> {
        let mut world: Vec<Affine> = Vec::with_capacity(self.bones.len());
        for bone in &self.bones {
            let local = bone.local();
            let m = match bone.parent.and_then(|p| world.get(p)) {
                Some(parent) => mul_affine(parent, &local),
                None => local,
            };
            world.push(m);
        }
        world
    }

    /// Parses the skeleton at the start of a skinned level of detail.
    fn parse(r: &mut Reader) -> Result<Self> {
        r.c_string()?; // the mesh name
        let _one = r.array::<1>()?[0];
        r.bytes(40)?; // bounding sphere and box, as in the info block
        let helper_count = r.array().map(u16::from_le_bytes)?;
        let dummy_count = r.array().map(u16::from_le_bytes)?;
        let helper_names = r.u32()? as usize;
        let _zero = r.array().map(u16::from_le_bytes)?;
        optional_stream(r, usize::from(helper_count) * 20)?;
        optional_stream(r, usize::from(dummy_count) * 56)?;
        optional_stream(r, helper_names)?;
        let _unknown = [r.u32()?, r.u32()?];
        let count = r.u32()? as usize;
        let names_len = r.u32()? as usize;
        if count > 1024 {
            return Err(Error::Invalid {
                offset: r.pos() - 8,
                reason: format!("{count} bones"),
            });
        }
        r.bytes(5)?;
        let offsets = (0..count)
            .map(|_| r.array().map(u16::from_le_bytes))
            .collect::<Result<Vec<_>>>()?;
        let names = optional_stream(r, names_len)?;
        let hierarchy = optional_stream(r, count * 60)?;
        let poses = optional_stream(r, count * 48)?;
        let matrices = optional_stream(r, count * 64)?;
        r.bytes(48)?; // a 3x4 matrix, identity on the young Hero
        let f = |b: &[u8], at: usize| f32::from_le_bytes(b[at..at + 4].try_into().unwrap());
        let bones = (0..count)
            .map(|i| {
                let start = usize::from(offsets[i]);
                let end = names[start.min(names.len())..]
                    .iter()
                    .position(|&b| b == 0)
                    .map_or(names.len(), |n| start + n);
                let h = &hierarchy[i * 60..];
                let parent = i32::from_le_bytes(h[4..8].try_into().unwrap());
                let pose = &poses[i * 48..];
                let m = &matrices[i * 64..];
                Bone {
                    name: latin1(names.get(start..end).unwrap_or_default()),
                    parent: usize::try_from(parent).ok().filter(|&p| p < i),
                    inverse_bind: std::array::from_fn(|row| {
                        std::array::from_fn(|col| f(m, (row * 4 + col) * 4))
                    }),
                    rotation: std::array::from_fn(|k| f(pose, k * 4)),
                    translation: std::array::from_fn(|k| f(pose, 16 + k * 4)),
                    scale: std::array::from_fn(|k| f(pose, 32 + k * 4)),
                }
            })
            .collect();
        Ok(Self {
            helper_count,
            dummy_count,
            bones,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Chunk {
    /// First word of the chunk. Static meshes: 0 for the first chunk, other values
    /// (such as 3) for later ones. Skinned meshes: the material of every sub-mesh.
    pub node: u32,
    /// Vertices carry 4 bone indices and 4 weights after the position.
    pub skinned: bool,
    /// Physics blocks after the chunk, as (two unknown words, unpacked bytes): on
    /// the young Hero's hair one block pairing vertices with a rest length and a
    /// stiffness (hair or cloth physics); four on a washing line. Not decoded.
    pub physics: Vec<([u32; 2], Vec<u8>)>,
    pub sub_meshes: Vec<SubMesh>,
    pub vertex_count: u32,
    /// Flags selecting the vertex layout. Every layout starts with position, normal
    /// and texture coordinates; flag 16 stores the position as three floats instead
    /// of one packed word, and flag 2 adds 8 bytes not decoded yet. Seen: 4 (12
    /// bytes), 6 and 20 (20 bytes), 22 (28 bytes).
    pub vertex_format: u32,
    pub stride: u32,
    /// Positions map through `center + v * scale`; packed positions unpack to -1..1,
    /// float positions use scale 1 and center 0.
    pub position_scale: [f32; 3],
    pub position_center: [f32; 3],
    /// Vertices as stored, `stride` bytes each.
    pub vertices: Vec<u8>,
    pub indices: Vec<u16>,
}

impl Lod {
    fn parse(block: &[u8], name: &[u8]) -> Result<Self> {
        let mut r = Reader::new(block);
        let mut materials = Vec::new();
        while let Some(material) = Material::try_parse(block, r.pos())? {
            r.bytes(material.1)?;
            materials.push(material.0);
        }
        let mut chunks = Vec::new();
        loop {
            let mut chunk = Chunk::parse(&mut r, Layout::Static)?;
            chunk.validate(materials.len(), r.pos())?;
            chunk.physics = Chunk::parse_end(&mut r)?;
            chunks.push(chunk);
            let rest = &block[r.pos()..];
            if name.is_empty() || rest.starts_with(name) || rest.len() < 64 {
                break;
            }
        }
        Ok(Self {
            skeleton: None,
            materials,
            chunks,
        })
    }

    /// A skinned level of detail: skeleton, materials, then chunks to the end.
    fn parse_skinned(block: &[u8], layout: Layout) -> Result<Self> {
        let mut r = Reader::new(block);
        let skeleton = Skeleton::parse(&mut r)?;
        let mut materials = Vec::new();
        while let Some(material) = Material::try_parse(block, r.pos())? {
            r.bytes(material.1)?;
            materials.push(material.0);
        }
        let mut chunks = Vec::new();
        while r.pos() < block.len() {
            let mut chunk = Chunk::parse(&mut r, layout)?;
            chunk.validate(materials.len(), r.pos())?;
            chunk.physics = Chunk::parse_end(&mut r)?;
            chunks.push(chunk);
        }
        Ok(Self {
            skeleton: Some(skeleton),
            materials,
            chunks,
        })
    }
}

impl Chunk {
    /// After a chunk: a `u32` count of physics blocks (0 usually, 1 on the young
    /// Hero's hair, 4 on a washing line), each two `u32`s, a length and a stream.
    fn parse_end(r: &mut Reader) -> Result<Vec<([u32; 2], Vec<u8>)>> {
        let count = r.u32()?;
        if count > 64 {
            return Err(Error::Invalid {
                offset: r.pos() - 4,
                reason: format!("{count} physics blocks"),
            });
        }
        (0..count)
            .map(|_| {
                let words = [r.u32()?, r.u32()?];
                let len = r.u32()? as usize;
                Ok((words, stream(r, len)?))
            })
            .collect()
    }

    fn parse(r: &mut Reader, layout: Layout) -> Result<Self> {
        let skinned = layout == Layout::Skinned;
        let node = r.u32()?;
        let _zero = r.u32()?;
        r.bytes(20)?; // bounding sphere and one more float
        let sub_mesh_count = if skinned {
            let _unknown = r.u32()?;
            r.u32()?
        } else {
            let count = r.u32()?;
            let _zero = r.u32()?;
            count
        };
        let vertex_count = r.u32()?;
        let _triangles = r.u32()?;
        let index_count = r.u32()?;
        let vertex_format = r.u32()?;
        let _sub_meshes_again = r.u32()?;
        let _zero = r.u32()?;
        let mut sub_meshes = Vec::new();
        for _ in 0..sub_mesh_count {
            let primitive_count = r.u32()?;
            let first_index = r.u32()?;
            let raw_flags: [u8; 3] = r.array()?;
            let (material, bones) = if skinned {
                let _vertex_count = r.u32()?;
                let _unknown = r.array().map(u16::from_le_bytes)?;
                let _unknown = r.array::<1>()?[0];
                let count = r.array::<1>()?[0];
                (node, r.bytes(count.into())?.to_vec())
            } else {
                (r.u32()?, Vec::new())
            };
            sub_meshes.push(SubMesh {
                primitive_count,
                first_index,
                strip: raw_flags[0] == 1,
                material,
                raw_flags,
                bones,
            });
        }
        let mut f = || r.u32().map(f32::from_bits);
        let position_scale = [f()?, f()?, f()?];
        let _one = f()?;
        let position_center = [f()?, f()?, f()?];
        let _zero = r.u32()?;
        let stride = r.u32()?;
        let _zero = r.u32()?;
        let vertices = stream(r, vertex_count as usize * stride as usize)?;
        let index_bytes = stream(r, index_count as usize * 2)?;
        let indices = index_bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| u16::from_le_bytes(*b))
            .collect();
        Ok(Self {
            node,
            skinned,
            physics: Vec::new(),
            sub_meshes,
            vertex_count,
            vertex_format,
            stride,
            position_scale,
            position_center,
            vertices,
            indices,
        })
    }

    fn validate(&self, materials: usize, end: usize) -> Result<()> {
        let err = |reason: String| {
            Err(Error::Invalid {
                offset: end,
                reason,
            })
        };
        if self.stride < self.uv_offset() as u32 + 4 {
            return err(format!("vertex stride {}", self.stride));
        }
        let used: u32 = self.sub_meshes.iter().map(SubMesh::index_count).sum();
        if used as usize != self.indices.len() {
            return err(format!(
                "sub-meshes use {used} of {} indices",
                self.indices.len()
            ));
        }
        if let Some(&i) = self
            .indices
            .iter()
            .find(|&&i| u32::from(i) >= self.vertex_count)
        {
            return err(format!("index {i} beyond {} vertices", self.vertex_count));
        }
        if let Some(s) = self
            .sub_meshes
            .iter()
            .find(|s| s.material as usize >= materials)
        {
            return err(format!("material {} of {materials}", s.material));
        }
        Ok(())
    }

    fn vertex(&self, i: usize) -> &[u8] {
        &self.vertices[i * self.stride as usize..][..self.stride as usize]
    }

    fn float_positions(&self) -> bool {
        self.vertex_format & 0x10 != 0
    }

    fn position_size(&self) -> usize {
        if self.float_positions() { 12 } else { 4 }
    }

    fn normal_offset(&self) -> usize {
        self.position_size() + if self.skinned { 8 } else { 0 }
    }

    /// Per vertex: 4 bone indices into the sub-mesh's [`SubMesh::bones`] and 4
    /// weights out of 255 (they sum to 255). Stored indices are 3x these, likely
    /// register offsets of 3-row matrices. Empty for static meshes.
    pub fn bone_weights(&self) -> Vec<([u8; 4], [u8; 4])> {
        if !self.skinned {
            return Vec::new();
        }
        let o = self.position_size();
        (0..self.vertex_count as usize)
            .map(|i| {
                let v = self.vertex(i);
                (
                    std::array::from_fn(|k| v[o + k] / 3),
                    std::array::from_fn(|k| v[o + 4 + k]),
                )
            })
            .collect()
    }

    fn uv_offset(&self) -> usize {
        self.normal_offset() + 4
    }

    pub fn positions(&self) -> Vec<[f32; 3]> {
        (0..self.vertex_count as usize)
            .map(|i| {
                let v = self.vertex(i);
                let n = if self.float_positions() {
                    std::array::from_fn(|a| f32::from_bits(word(v, a * 4)))
                } else {
                    unpack_11_11_10(word(v, 0))
                };
                std::array::from_fn(|a| self.position_center[a] + n[a] * self.position_scale[a])
            })
            .collect()
    }

    pub fn normals(&self) -> Vec<[f32; 3]> {
        (0..self.vertex_count as usize)
            .map(|i| unpack_11_11_10(word(self.vertex(i), self.normal_offset())))
            .collect()
    }

    /// First texture coordinate set. Stored as `u16`; `(v - 0x4000) / 0x800`
    /// maps the values seen on simple quads to 0..1.
    pub fn uvs(&self) -> Vec<[f32; 2]> {
        (0..self.vertex_count as usize)
            .map(|i| {
                let v = self.vertex(i);
                let o = self.uv_offset();
                let c =
                    |o: usize| (f32::from(u16::from_le_bytes([v[o], v[o + 1]])) - 16384.0) / 2048.0;
                [c(o), c(o + 2)]
            })
            .collect()
    }

    /// Triangles of one sub-mesh, with strips unrolled and degenerate triangles dropped.
    pub fn triangles(&self, sub_mesh: &SubMesh) -> Vec<[u16; 3]> {
        let start = sub_mesh.first_index as usize;
        let idx = &self.indices[start..start + sub_mesh.index_count() as usize];
        if !sub_mesh.strip {
            return idx.as_chunks::<3>().0.to_vec();
        }
        idx.windows(3)
            .enumerate()
            .map(|(k, w)| {
                if k % 2 == 0 {
                    [w[0], w[1], w[2]]
                } else {
                    [w[1], w[0], w[2]]
                }
            })
            .filter(|t| t[0] != t[1] && t[1] != t[2] && t[0] != t[2])
            .collect()
    }
}

impl Material {
    /// Parses a material at `at` if one starts there: a small id, then a printable,
    /// non-empty NUL-terminated name. Returns it with its length in bytes.
    fn try_parse(block: &[u8], at: usize) -> Result<Option<(Self, usize)>> {
        let Some(id) = block
            .get(at..at + 4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
        else {
            return Ok(None);
        };
        let rest = &block[at + 4..];
        let Some(len) = rest.iter().take(128).position(|&b| b == 0) else {
            return Ok(None);
        };
        let name = &rest[..len];
        if id > 1000 || name.is_empty() || !name.iter().all(|b| (32..127).contains(b)) {
            return Ok(None);
        }
        let raw: [u8; 33] = rest
            .get(len + 1..len + 34)
            .ok_or(Error::UnexpectedEof {
                offset: at + 4 + len + 1,
                wanted: 33,
            })?
            .try_into()
            .unwrap();
        let word = |o: usize| u32::from_le_bytes(raw[o..o + 4].try_into().unwrap());
        Ok(Some((
            Self {
                id,
                name: latin1(name),
                texture: word(4),
                second_texture: word(8),
                raw,
            },
            4 + len + 1 + 33,
        )))
    }
}

/// A [`stream`] of `len` bytes, absent entirely when `len` is 0.
fn optional_stream(r: &mut Reader, len: usize) -> Result<Vec<u8>> {
    if len == 0 {
        Ok(Vec::new())
    } else {
        stream(r, len)
    }
}

/// A `u16` length (or `0xffff` and a `u32` length). Zero means `len` raw bytes
/// follow; otherwise an LZO1X stream gives `len - 3` bytes and the last 3 follow raw.
fn stream(r: &mut Reader, len: usize) -> Result<Vec<u8>> {
    // Nothing in a mesh unpacks to more than this; a larger length means the
    // bytes were misread, and must not reach an allocation.
    const MAX_LEN: usize = 64 << 20;
    if len > MAX_LEN {
        return Err(Error::Invalid {
            offset: r.pos(),
            reason: format!("stream of {len} bytes"),
        });
    }
    let short = u16::from_le_bytes(r.array()?);
    let stream_len = if short == 0xffff {
        r.u32()? as usize
    } else {
        short.into()
    };
    if stream_len == 0 {
        return Ok(r.bytes(len)?.to_vec());
    }
    let mut out = lzo::decompress(r.bytes(stream_len)?, len.saturating_sub(3))?;
    out.extend_from_slice(r.bytes(len.min(3))?);
    Ok(out)
}

fn word(v: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(v[at..at + 4].try_into().unwrap())
}

/// Signed 11-bit x (bits 0–10), 11-bit y (11–21) and 10-bit z (22–31), each
/// scaled to -1..1.
fn unpack_11_11_10(p: u32) -> [f32; 3] {
    let signed = |v: u32, bits: u32| -> f32 {
        let shift = 32 - bits;
        let s = ((v << shift) as i32) >> shift;
        s as f32 / ((1 << (bits - 1)) - 1) as f32
    };
    [
        signed(p & 0x7ff, 11),
        signed((p >> 11) & 0x7ff, 11),
        signed(p >> 22, 10),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unpacks_corners() {
        // From MESH_TG_CIRCLE_CHALK_01: (+1, +1, 0), (-1, -1, 0), and normal (0, 0, +1).
        assert_eq!(unpack_11_11_10(0x001f_fbff), [1.0, 1.0, 0.0]);
        assert_eq!(unpack_11_11_10(0x0020_0c01), [-1.0, -1.0, 0.0]);
        assert_eq!(unpack_11_11_10(0x7fc0_0000), [0.0, 0.0, 1.0]);
    }

    /// The level-of-detail block of `MESH_TG_CIRCLE_CHALK_01`, rebuilt by hand with
    /// raw (uncompressed) vertex and index data.
    fn quad_lod() -> Vec<u8> {
        let mut b = Vec::new();
        let u32s = |b: &mut Vec<u8>, v: &[u32]| {
            v.iter().for_each(|x| b.extend_from_slice(&x.to_le_bytes()))
        };
        let f32s = |b: &mut Vec<u8>, v: &[f32]| {
            v.iter().for_each(|x| b.extend_from_slice(&x.to_le_bytes()))
        };
        u32s(&mut b, &[0]);
        b.extend_from_slice(b"7 - Default\0");
        let mut raw = [0u8; 33];
        raw[4..8].copy_from_slice(&3938u32.to_le_bytes());
        b.extend_from_slice(&raw);
        u32s(&mut b, &[0, 0]);
        f32s(&mut b, &[0.0, 0.0, 0.0, 6.36, 0.11]);
        u32s(&mut b, &[1, 0, 4, 2, 6, 4, 1, 0]);
        u32s(&mut b, &[2, 0]);
        b.extend_from_slice(&[0, 0, 0]);
        u32s(&mut b, &[0]);
        f32s(&mut b, &[4.5, 4.5, 0.0, 1.0, 0.0, 0.0, 0.5]);
        u32s(&mut b, &[0, 12, 0]);
        b.extend_from_slice(&[0, 0]);
        for (p, uv) in [
            (0x001f_fbff_u32, [0x4800u16, 0x4000]),
            (0x0020_0c01, [0x4000, 0x4800]),
            (0x001f_fc01, [0x4000, 0x4000]),
            (0x0020_0bff, [0x4800, 0x4800]),
        ] {
            u32s(&mut b, &[p, 0x7fc0_0000]);
            uv.iter()
                .for_each(|x| b.extend_from_slice(&x.to_le_bytes()));
        }
        b.extend_from_slice(&[0, 0]);
        [0u16, 3, 1, 0, 1, 2]
            .iter()
            .for_each(|x| b.extend_from_slice(&x.to_le_bytes()));
        u32s(&mut b, &[0]);
        b
    }

    /// A second chunk with float positions (format 20, stride 20), one triangle.
    fn float_chunk() -> Vec<u8> {
        let mut b = Vec::new();
        let u32s = |b: &mut Vec<u8>, v: &[u32]| {
            v.iter().for_each(|x| b.extend_from_slice(&x.to_le_bytes()))
        };
        let f32s = |b: &mut Vec<u8>, v: &[f32]| {
            v.iter().for_each(|x| b.extend_from_slice(&x.to_le_bytes()))
        };
        u32s(&mut b, &[3, 0]);
        f32s(&mut b, &[0.0; 5]);
        u32s(&mut b, &[1, 0, 3, 1, 3, 20, 1, 0]);
        u32s(&mut b, &[1, 0]);
        b.extend_from_slice(&[0, 0, 0]);
        u32s(&mut b, &[0]);
        f32s(&mut b, &[1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0]);
        u32s(&mut b, &[0, 20, 0]);
        b.extend_from_slice(&[0, 0]);
        for p in [
            [10.0f32, 20.0, 30.0],
            [11.0, 20.0, 30.0],
            [10.0, 21.0, 30.0],
        ] {
            f32s(&mut b, &p);
            u32s(&mut b, &[0x7fc0_0000, 0x4000_4000]);
        }
        b.extend_from_slice(&[0, 0]);
        [0u16, 1, 2]
            .iter()
            .for_each(|x| b.extend_from_slice(&x.to_le_bytes()));
        u32s(&mut b, &[0]);
        b
    }

    #[test]
    fn parses_quad() {
        let lod = Lod::parse(&quad_lod(), b"").unwrap();
        assert_eq!(lod.materials.len(), 1);
        assert_eq!(
            (lod.materials[0].name.as_str(), lod.materials[0].texture),
            ("7 - Default", 3938)
        );
        let c = &lod.chunks[0];
        assert_eq!(c.positions()[0], [4.5, 4.5, 0.5]);
        assert_eq!(c.positions()[1], [-4.5, -4.5, 0.5]);
        assert_eq!(c.normals()[0], [0.0, 0.0, 1.0]);
        assert_eq!(c.uvs()[0], [1.0, 0.0]);
        assert_eq!(c.triangles(&c.sub_meshes[0]), [[0, 3, 1], [0, 1, 2]]);
    }

    #[test]
    fn rest_pose_composes_parent_first() {
        // Root at (0, 0, 100); child 10 units along the root's x, root turned 90°
        // about z (stored as the inverse rotation: quaternion (0, 0, -s, s)).
        let s = std::f32::consts::FRAC_1_SQRT_2;
        let bone = |parent, rotation, translation| Bone {
            name: String::new(),
            parent,
            inverse_bind: [[0.0; 4]; 3],
            rotation,
            translation,
            scale: [1.0; 3],
        };
        let skeleton = Skeleton {
            helper_count: 0,
            dummy_count: 0,
            bones: vec![
                bone(None, [0.0, 0.0, -s, s], [0.0, 0.0, 100.0]),
                bone(Some(0), [0.0, 0.0, 0.0, 1.0], [10.0, 0.0, 0.0]),
            ],
        };
        let pose = skeleton.rest_pose();
        let child = [pose[1][0][3], pose[1][1][3], pose[1][2][3]];
        assert!(
            (child[0]).abs() < 1e-4 && (child[1] - 10.0).abs() < 1e-4,
            "{child:?}"
        );
        assert_eq!(child[2], 100.0);
    }

    #[test]
    fn unrolls_strips() {
        let mut lod = Lod::parse(&quad_lod(), b"").unwrap();
        let chunk = &mut lod.chunks[0];
        chunk.indices = vec![0, 1, 2, 2, 3, 0];
        let strip = SubMesh {
            primitive_count: 4,
            first_index: 0,
            strip: true,
            material: 0,
            raw_flags: [1, 0, 0],
            bones: Vec::new(),
        };
        assert_eq!(chunk.triangles(&strip), [[0, 1, 2], [3, 2, 0]]);
    }

    #[test]
    fn reads_chunks_until_the_trailer() {
        let mut block = quad_lod();
        block.extend(float_chunk());
        block.extend_from_slice(b"MESH_TEST\0");
        block.resize(block.len() + 140, 0);
        let lod = Lod::parse(&block, b"MESH_TEST").unwrap();
        assert_eq!(lod.chunks.len(), 2);
        let c = &lod.chunks[1];
        assert_eq!((c.node, c.stride), (3, 20));
        assert_eq!(c.positions()[1], [11.0, 20.0, 30.0]);
        assert_eq!(c.normals()[0], [0.0, 0.0, 1.0]);
        assert_eq!(c.uvs()[0], [0.0, 0.0]);
    }
}
