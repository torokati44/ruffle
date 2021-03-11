use crate::avm2::{
    Activation as Avm2Activation, Error as Avm2Error, Namespace as Avm2Namespace,
    Object as Avm2Object, QName as Avm2QName, StageObject as Avm2StageObject,
    TObject as Avm2TObject,
};
use crate::backend::render::BitmapFormat;
use crate::{avm1::Object as Avm1Object, backend::render::BitmapHandle};
pub use crate::{library::MovieLibrary, transform::Transform, Color};
use std::fs::File;
use std::io::Write;
use crate::backend::render::ShapeHandle;
use crate::context::{RenderContext, UpdateContext};
use crate::display_object::{DisplayObjectBase, TDisplayObject};
use crate::prelude::*;
use crate::tag_utils::SwfMovie;
use crate::types::{Degrees, Percent};
use crate::vminterface::{AvmType, Instantiator};
use gc_arena::{Collect, GcCell};
use std::sync::Arc;

#[derive(Clone, Debug, Collect, Copy)]
#[collect(no_drop)]
pub struct Graphic<'gc>(GcCell<'gc, GraphicData<'gc>>);

#[derive(Clone, Debug, Collect)]
#[collect(no_drop)]
pub struct GraphicData<'gc> {
    base: DisplayObjectBase<'gc>,
    static_data: gc_arena::Gc<'gc, GraphicStatic>,
    avm2_object: Option<Avm2Object<'gc>>,
    proxy_bitmap: Option<BitmapHandle>,
}

impl<'gc> Graphic<'gc> {
    pub fn from_swf_tag(
        context: &mut UpdateContext<'_, 'gc, '_>,
        swf_shape: swf::Shape,
        movie: Arc<SwfMovie>,
    ) -> Self {
        let library = context.library.library_for_movie(movie);
        let static_data = GraphicStatic {
            id: swf_shape.id,
            bounds: swf_shape.shape_bounds.clone().into(),
            render_handle: context
                .renderer
                .register_shape((&swf_shape).into(), library),
            shape: swf_shape,
        };
        Graphic(GcCell::allocate(
            context.gc_context,
            GraphicData {
                base: Default::default(),
                static_data: gc_arena::Gc::allocate(context.gc_context, static_data),
                avm2_object: None,
                proxy_bitmap: None,
            },
        ))
    }
}

impl<'gc> TDisplayObject<'gc> for Graphic<'gc> {
    impl_display_object!(base);

    fn id(&self) -> CharacterId {
        self.0.read().static_data.id
    }

    fn self_bounds(&self) -> BoundingBox {
        self.0.read().static_data.bounds.clone()
    }

    fn world_bounds(&self) -> BoundingBox {
        // TODO: Use dirty flags and cache this.
        let mut bounds = self.local_bounds();
        let mut node = self.parent();
        while let Some(display_object) = node {
            bounds = bounds.transform(&*display_object.matrix());
            node = display_object.parent();
        }
        bounds
    }

    fn run_frame(&self, context: &mut UpdateContext<'_, 'gc, '_>) {

        context.renderer.set_offscreen_viewport_dimensions(128, 128);


        context
            .renderer
            .begin_frame_offscreen(Color::from_rgb(0, 0));


        let mut write = self.0.write(context.gc_context);
        let opbm = write.proxy_bitmap;
        write.proxy_bitmap = None;
        drop(write);



        let mut view_bounds = self.world_bounds();
        // let mut view_bounds = BoundingBox::default();

        // view_bounds.set_width(Twips::from_pixels(512.0));
        // view_bounds.set_height(Twips::from_pixels(512.0));


        let mut transform_stack = crate::transform::TransformStack::new();
        let mut mx = self.local_to_global_matrix();
        mx.tx -= view_bounds.x_min;
        mx.ty -= view_bounds.y_min;

        view_bounds.set_x(Twips::from_pixels(0.0));
        view_bounds.set_y(Twips::from_pixels(0.0));

        //view_bounds.x_min = Twips::from_pixels(0.0);
        //view_bounds.y_min = Twips::from_pixels(0.0);
        transform_stack.push(&crate::transform::Transform {
            matrix: mx,
            ..Default::default()
        });


        let mut render_context = RenderContext {
            renderer: context.renderer,
            library: &context.library,
            transform_stack: &mut transform_stack,
            view_bounds,
            clip_depth_stack: vec![],
            allow_mask: true,
        };

        self.render(&mut render_context, false);

        let bm = context.renderer.end_frame_offscreen().unwrap();
        let mut bmd = match bm.data {
            BitmapFormat::Rgb(x) => x,
            BitmapFormat::Rgba(x) => x,
        };

        //for i in 400..bmd.len() {
        //    bmd[(i as isize -400) as usize] += bmd[i];
        //}

        let mut write = self.0.write(context.gc_context);

        //let mut file = File::create(format!("file-{:#?}.rgba", self.as_ptr())).unwrap();
        //file.write_all(&bmd);

        match opbm {
            Some(bmh) => {
                let nbmh = context
                    .renderer
                    .update_texture(bmh, bm.width, bm.height, bmd)
                    .unwrap();
                write.proxy_bitmap = Some(nbmh);
            }
            None => {
                let nbmh = context
                    .renderer
                    .register_bitmap_raw(bm.width, bm.height, bmd)
                    .unwrap();
                write.proxy_bitmap = Some(nbmh);
            }
        };
    }

    fn render_self(&self, context: &mut RenderContext<'_, 'gc>) {

        let read = self.0.read();
        match read.proxy_bitmap {
            Some(bmh) => {
                println!("rendering bitmap");
                let mut tx = Transform::default();
                tx.matrix.tx += self.world_bounds().x_min;
                tx.matrix.ty += self.world_bounds().y_min;
                context
                    .renderer
                    .render_bitmap(bmh, &tx, false);
            }
            None => {
                println!("rendering for real");

                context.renderer.render_shape(
                    self.0.read().static_data.render_handle,
                    context.transform_stack.transform(),
                );
            }
        }

        drop(read);
    }

    fn hit_test_shape(
        &self,
        _context: &mut UpdateContext<'_, 'gc, '_>,
        point: (Twips, Twips),
    ) -> bool {
        // Transform point to local coordinates and test.
        if self.world_bounds().contains(point) {
            let local_matrix = self.global_to_local_matrix();
            let point = local_matrix * point;
            let shape = &self.0.read().static_data.shape;
            crate::shape_utils::shape_hit_test(shape, point, &local_matrix)
        } else {
            false
        }
    }

    fn post_instantiation(
        &self,
        context: &mut UpdateContext<'_, 'gc, '_>,
        display_object: DisplayObject<'gc>,
        _init_object: Option<Avm1Object<'gc>>,
        _instantiated_by: Instantiator,
        run_frame: bool,
    ) {
        if self.vm_type(context) == AvmType::Avm2 {
            let mut allocator = || {
                let mut activation = Avm2Activation::from_nothing(context.reborrow());
                let mut proto = activation.context.avm2.prototypes().shape;
                let constr = proto
                    .get_property(
                        proto,
                        &Avm2QName::new(Avm2Namespace::public(), "constructor"),
                        &mut activation,
                    )?
                    .coerce_to_object(&mut activation)?;

                let object = Avm2StageObject::for_display_object(
                    activation.context.gc_context,
                    display_object,
                    proto,
                )
                .into();
                constr.call(Some(object), &[], &mut activation, Some(proto))?;

                Ok(object)
            };
            let result: Result<Avm2Object<'gc>, Avm2Error> = allocator();

            match result {
                Ok(object) => self.0.write(context.gc_context).avm2_object = Some(object),
                Err(e) => log::error!("Got {} when constructing AVM2 side of display object", e),
            }
        }

        if run_frame {
            self.run_frame(context);
        }
    }

    fn object2(&self) -> Avm2Value<'gc> {
        self.0
            .read()
            .avm2_object
            .map(Avm2Value::from)
            .unwrap_or(Avm2Value::Undefined)
    }
}

/// Static data shared between all instances of a graphic.
#[allow(dead_code)]
#[derive(Collect)]
#[collect(require_static)]
struct GraphicStatic {
    id: CharacterId,
    shape: swf::Shape,
    render_handle: ShapeHandle,
    bounds: BoundingBox,
}
