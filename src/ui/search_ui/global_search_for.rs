use super::*;

impl QuillApp {
    /// The global search for `query` (a clicked hashtag outside its
    /// group), in the main window ([`Self::open_search_ui`]).
    pub(in crate::ui) fn open_global_search_for(
        &mut self,
        query: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let asked = query.to_string();
        if self.hand_to_main_window(cx, move |app, window, cx| {
            app.open_global_search_for(&asked, window, cx)
        }) {
            return;
        }
        self.open_search_ui(window, cx);
        self.search_ui
            .input
            .update(cx, |input, cx| input.set_value(query, window, cx));
        self.sync_search_query(query, cx);
    }
}
