use std::borrow::Cow;
use std::sync::Arc;

use crate::backend::{RenderBackend, ShapeHandle, ShapeHandleImpl, ViewportDimensions};
use crate::bitmap::{Bitmap, BitmapHandle, BitmapHandleImpl, BitmapSize, BitmapSource, SyncHandle};
use crate::commands::CommandList;
use crate::error::Error;
use crate::quality::StageQuality;
use crate::shape_utils::DistilledShape;
use gc_arena::MutationContext;
use swf::Color;

use super::{Context3D, Context3DCommand};

pub struct NullBitmapSource;

impl BitmapSource for NullBitmapSource {
    fn bitmap_size(&self, _id: u16) -> Option<BitmapSize> {
        None
    }
    fn bitmap_handle(&self, _id: u16, _renderer: &mut dyn RenderBackend) -> Option<BitmapHandle> {
        None
    }
}

pub struct NullRenderer {
    dimensions: ViewportDimensions,
}

impl NullRenderer {
    pub fn new(dimensions: ViewportDimensions) -> Self {
        Self { dimensions }
    }
}

#[derive(Clone, Debug)]
struct NullBitmapHandle;
impl BitmapHandleImpl for NullBitmapHandle {}

#[derive(Clone, Debug)]
struct NullShapeHandle;
impl ShapeHandleImpl for NullShapeHandle {}

impl RenderBackend for NullRenderer {
    fn viewport_dimensions(&self) -> ViewportDimensions {
        self.dimensions
    }
    fn set_viewport_dimensions(&mut self, dimensions: ViewportDimensions) {
        self.dimensions = dimensions;
    }
    fn register_shape(
        &mut self,
        _shape: DistilledShape,
        _bitmap_source: &dyn BitmapSource,
    ) -> ShapeHandle {
        ShapeHandle(Arc::new(NullShapeHandle))
    }
    fn register_glyph_shape(&mut self, _shape: &swf::Glyph) -> ShapeHandle {
        ShapeHandle(Arc::new(NullShapeHandle))
    }

    fn render_offscreen(
        &mut self,
        _handle: BitmapHandle,
        _width: u32,
        _height: u32,
        _commands: CommandList,
        _quality: StageQuality,
    ) -> Option<Box<dyn SyncHandle>> {
        None
    }

    fn submit_frame(&mut self, _clear: Color, _commands: CommandList) {}
    fn register_bitmap(&mut self, _bitmap: Bitmap) -> Result<BitmapHandle, Error> {
        Ok(BitmapHandle(Arc::new(NullBitmapHandle)))
    }

    fn update_texture(&mut self, _handle: &BitmapHandle, _bitmap: Bitmap) -> Result<(), Error> {
        Ok(())
    }

    fn create_context3d(&mut self) -> Result<Box<dyn super::Context3D>, Error> {
        Err(Error::Unimplemented("createContext3D".into()))
    }

    fn context3d_present<'gc>(
        &mut self,
        _context: &mut dyn Context3D,
        _commands: Vec<Context3DCommand<'gc>>,
        _mc: MutationContext<'gc, '_>,
    ) -> Result<(), Error> {
        Err(Error::Unimplemented("Context3D.present".into()))
    }

    fn debug_info(&self) -> Cow<'static, str> {
        Cow::Borrowed("Renderer: Null")
    }

    fn set_quality(&mut self, _quality: StageQuality) {}
}
