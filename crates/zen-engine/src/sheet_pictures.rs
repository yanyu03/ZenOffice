//! Pictures float over cells, move with their anchor and retain a fixed display size.
use super::{CellCoord, Sheet};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PicturePlacement {
    pub anchor: CellCoord,
    pub offset: [f32; 2],
    pub size: [f32; 2],
}

#[derive(Debug, Clone)]
pub struct SheetPicture {
    pub id: u64,
    pub name: String,
    pub pixels: Arc<Vec<u8>>,
    pub dimensions: [u32; 2],
    pub placement: PicturePlacement,
}

impl Sheet {
    pub fn pictures(&self) -> &[SheetPicture] {
        &self.pictures
    }
    pub fn picture(&self, id: u64) -> Option<&SheetPicture> {
        self.pictures.iter().find(|picture| picture.id == id)
    }
    pub fn validate_picture_placement(
        &self,
        placement: PicturePlacement,
    ) -> Result<(), &'static str> {
        if placement.anchor.col >= self.max_cols
            || placement.anchor.row >= self.max_rows
            || placement
                .offset
                .iter()
                .any(|v| !v.is_finite() || *v < 0.0 || *v > 4096.0)
            || placement
                .size
                .iter()
                .any(|v| !v.is_finite() || *v < 1.0 || *v > 4096.0)
        {
            return Err("图片的位置或尺寸超出当前支持范围");
        }
        Ok(())
    }
    pub fn insert_picture(
        &mut self,
        name: String,
        pixels: Vec<u8>,
        dimensions: [u32; 2],
        placement: PicturePlacement,
    ) -> Result<u64, &'static str> {
        self.validate_picture_placement(placement)?;
        let area = dimensions[0] as u64 * dimensions[1] as u64;
        if area == 0
            || dimensions.iter().any(|side| *side > 2048)
            || pixels.len() as u64 != area * 4
        {
            return Err("图片像素数据无效");
        }
        let total: u64 = self
            .pictures
            .iter()
            .map(|p| p.dimensions[0] as u64 * p.dimensions[1] as u64)
            .sum();
        if self.pictures.len() >= 32 || total + area > 16_777_216 {
            return Err("当前工作表最多支持 32 张图片、合计 1600 万像素");
        }
        let id = self.next_picture_id;
        self.next_picture_id = id.checked_add(1).ok_or("图片编号已耗尽")?;
        self.pictures.push(SheetPicture {
            id,
            name,
            pixels: Arc::new(pixels),
            dimensions,
            placement,
        });
        Ok(id)
    }
    pub fn set_picture_placement(
        &mut self,
        id: u64,
        placement: PicturePlacement,
    ) -> Result<(), &'static str> {
        self.validate_picture_placement(placement)?;
        let picture = self
            .pictures
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or("找不到此图片")?;
        picture.placement = placement;
        Ok(())
    }
    pub fn remove_picture(&mut self, id: u64) {
        self.pictures.retain(|p| p.id != id);
    }
    pub fn picture_layer(&mut self, id: u64, front: bool) {
        if let Some(index) = self.pictures.iter().position(|p| p.id == id) {
            let picture = self.pictures.remove(index);
            if front {
                self.pictures.push(picture);
            } else {
                self.pictures.insert(0, picture);
            }
        }
    }
}
