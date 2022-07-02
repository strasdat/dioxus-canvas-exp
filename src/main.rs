use dioxus_core as dioxus;
use dioxus_core::prelude::*;
use dioxus_core_macro::*;
use dioxus_hooks::*;
use dioxus_html as dioxus_elements;
use piet::kurbo::BezPath;
use piet::kurbo::Circle;
use piet::kurbo::Rect;
use piet::kurbo::Shape;
use piet::Color;
use piet::PaintBrush;
use piet::RenderContext;
use std::future;
use std::rc::Rc;
use std::sync::Mutex;
use wasm_bindgen::JsCast;
use web_sys::window;
use web_sys::CanvasRenderingContext2d;
use web_sys::HtmlCanvasElement;

const TOLERANCE: f64 = 0.1;

// each platform could export a Canvas
pub mod web {
    use super::*;

    pub fn Canvas<'a>(cx: Scope<'a, CanvasProps<'a>>) -> Element<'a> {
        GenericCanvas::<WebHandler>(cx)
    }

    pub fn Circle(cx: Scope<CircleProps>) -> Element<'_> {
        log::info!("circle initialized");
        GenericCircle::<WebHandler>(cx)
    }
}

#[derive(Props)]
pub struct CanvasProps<'a> {
    children: Element<'a>,
}

pub fn GenericCanvas<'a, C: CanvasHandler + 'static>(
    cx: Scope<'a, CanvasProps<'a>>,
) -> Element<'a> {
    let id = cx.scope_id();
    let canvas: CanvasHandle<C> = cx.provide_context(CanvasHandle::new(id));
    let canvas_clone = canvas.clone();
    use_future(&cx, (), move |_| async move {
        // futures will not be polled until after the first render in the web renderer...
        future::ready(()).await;
        canvas_clone.onmount(id);
        log::info!("Canvas {} initialized", id.0);
    });
    cx.render(rsx! {
        {
            [
                &C::create(id).map(|lzy| cx.render(lzy)).flatten(),
            ]
        }
        {
            [
                &cx.props.children
            ]
        }
    })
}

/// A handle to the canvas
pub struct CanvasHandle<C: CanvasHandler>(Rc<Mutex<Canvas<C>>>);

impl<C: CanvasHandler> Clone for CanvasHandle<C> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<C: CanvasHandler> CanvasHandle<C> {
    fn new(id: ScopeId) -> Self {
        let canvas = Canvas::new(id);
        Self(Rc::new(Mutex::new(canvas)))
    }

    fn onmount(&self, id: ScopeId) {
        let mut canvas = self.0.lock().unwrap();
        canvas.onmount(id);
    }

    fn draw(&self, shape: impl Shape, brush: impl Into<PaintBrush>, width: f64) {
        let mut canvas = self.0.lock().unwrap();
        canvas.push(CanvasCommand::Draw(
            shape.into_path(TOLERANCE),
            brush.into(),
            width,
        ));
    }

    fn clear(&self, rect: Rect, color: Color) {
        let mut canvas = self.0.lock().unwrap();
        canvas.push(CanvasCommand::Clear(rect, color));
    }
}

pub struct Canvas<C: CanvasHandler> {
    key: ScopeId,
    lzy: Option<C>,
    command_queue: Vec<CanvasCommand>,
}

impl<C: CanvasHandler> Canvas<C> {
    fn new(key: ScopeId) -> Canvas<C> {
        Canvas {
            key: key,
            lzy: None,
            command_queue: Vec::new(),
        }
    }

    fn push(&mut self, command: CanvasCommand) {
        if let Some(c) = &mut self.lzy {
            let draw = c.draw();
            command.draw(draw);
        } else {
            log::info!("Creating {:?}", command);
            self.command_queue.push(command);
        }
    }

    fn onmount(&mut self, id: ScopeId) {
        let mut new = C::onmount(id);
        // draw any queued commands
        {
            let draw = new.draw();
            for cmd in self.command_queue.drain(..) {
                cmd.draw(draw);
            }
        }
        self.lzy = Some(new);
    }
}

#[derive(Debug)]
enum CanvasCommand {
    Draw(BezPath, PaintBrush, f64),
    Clear(Rect, Color),
    // more commands here
}

impl CanvasCommand {
    fn draw<R: RenderContext>(self, ctx: &mut R) {
        log::info!("drawing: {:?}", self);
        match self {
            CanvasCommand::Draw(path, brush, width) => ctx.stroke(path, &brush, width),
            CanvasCommand::Clear(rect, color) => ctx.clear(rect, color),
        }
    }
}

pub trait CanvasHandler {
    type RenderContext: piet::RenderContext;

    fn create<'a, 'b>(id: ScopeId) -> Option<LazyNodes<'a, 'b>>;

    fn onmount(id: ScopeId) -> Self;

    fn draw(&mut self) -> &mut Self::RenderContext;

    // could add more methods here to handle filters, etc.
}

struct WebHandler {
    render_ctx: piet_web::WebRenderContext<'static>,
}

impl CanvasHandler for WebHandler {
    type RenderContext = piet_web::WebRenderContext<'static>;

    fn create<'b, 'c>(id: ScopeId) -> Option<LazyNodes<'b, 'c>> {
        Some(rsx! {
            canvas{
                id: "dioxus-canvas-{id.0}"
            }
        })
    }

    fn onmount(id: ScopeId) -> WebHandler {
        let window = window().unwrap();
        let canvas = window
            .document()
            .unwrap()
            .get_element_by_id(&format!("dioxus-canvas-{}", id.0))
            .unwrap();
        let canvas_html: HtmlCanvasElement = canvas.dyn_into().unwrap();
        let context: CanvasRenderingContext2d = canvas_html
            .get_context("2d")
            .unwrap()
            .unwrap()
            .dyn_into()
            .unwrap();
        let context = piet_web::WebRenderContext::new(context, window);
        log::info!("Web Canvas {} initialized", id.0);
        Self {
            render_ctx: context,
        }
    }

    fn draw(&mut self) -> &mut Self::RenderContext {
        &mut self.render_ctx
    }
}

// real elements would have some optional props here
#[derive(Props, PartialEq)]
pub struct CircleProps {
    x: f64,
    y: f64,
    radius: f64,
}

pub fn GenericCircle<C: CanvasHandler + 'static>(cx: Scope<CircleProps>) -> Element {
    let maybe_context: Option<CanvasHandle<C>>= cx.consume_context();
    if maybe_context.is_none(){
        log::info!("context is none :(");
      return None;
    }
    log::info!("context :)");

    let canvas: CanvasHandle<C> = maybe_context.unwrap();
    let CircleProps { x, y, radius } = cx.props;
    canvas.draw(
        Circle::new((*x, *y), *radius),
        PaintBrush::Color(Color::RED),
        10.0,
    );
    None
}

fn main() {
    // init debug tool for WebAssembly
    wasm_logger::init(wasm_logger::Config::default());
    console_error_panic_hook::set_once();

    dioxus_web::launch(app);
}

fn app(cx: Scope) -> Element {
    cx.render(rsx! {
          web::Circle{x: 10f64, y:10f64, radius:5f64}
    })
}