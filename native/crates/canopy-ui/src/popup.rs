use egui::{Order, Plugin, Ui};

pub(crate) struct FinishPopupLayout;

impl Plugin for FinishPopupLayout {
    fn debug_name(&self) -> &'static str {
        "Canopy popup layout"
    }

    fn on_end_pass(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx();
        if !ctx.any_popup_open() {
            return;
        }
        let layers = ctx.memory(|memory| memory.areas().visible_layer_ids());
        let sizing_popup = layers.into_iter().any(|layer| {
            layer.order == Order::Foreground
                && ctx
                    .read_response(layer.id)
                    .is_some_and(|response| response.layer_id == layer && !response.visible())
        });
        if sizing_popup {
            // egui first lays out a popup in an invisible Area. Show the measured
            // layout in this frame instead of waiting for a separate native redraw.
            ctx.request_discard("Measure Canopy popup");
        }
    }
}
