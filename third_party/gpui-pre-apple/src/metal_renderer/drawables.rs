//! QUILL: which drawable a frame draws into (docs/decisions/glass-display-link.md).
//!
//! While a `CAMetalDisplayLink` drives the layer, `-[CAMetalLayer
//! nextDrawable]` throws `CAMetalLayerInvalidOperation`. The exception
//! unwinds through the GPUI frame and drops the window. A frame in a link's
//! update draws into the drawable the link handed out, whatever it is; a
//! drawable it can't use well is drawn as best it can, and the frame is
//! presented again in the link's next update.

/// Where the frame's drawable came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DrawableSource {
    /// A display link's update: the layer must not be asked for another.
    Link,
    /// The layer's `nextDrawable`, with no display link attached.
    Layer,
}

/// What a frame does with a drawable that can't serve it as it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Swap {
    /// The drawable serves the frame.
    Keep,
    /// Ask the layer for its next drawable.
    AskLayer,
    /// Draw what can be drawn into it (glass flat, or nothing), and present
    /// the scene again in the link's next update.
    DrawAgainNext,
}

impl DrawableSource {
    /// A drawable whose pixel format the pipelines can't draw, after the
    /// layer has left extended range.
    pub(super) fn wrong_format(self) -> Swap {
        match self {
            Self::Layer => Swap::AskLayer,
            Self::Link => Swap::DrawAgainNext,
        }
    }

    /// A frame with glass needs a drawable it can read back.
    pub(super) fn for_glass(self, has_glass: bool, framebuffer_only: bool) -> Swap {
        match (has_glass && framebuffer_only, self) {
            (false, _) => Swap::Keep,
            (true, Self::Layer) => Swap::AskLayer,
            (true, Self::Link) => Swap::DrawAgainNext,
        }
    }
}

/// Whether the screen shows the last scene as GPUI drew it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Presented {
    #[default]
    AsDrawn,
    /// Skipped, or drawn with its glass flat, in a link's update: the next
    /// update presents the scene again.
    Owed,
}

impl From<Swap> for Presented {
    fn from(swap: Swap) -> Self {
        match swap {
            Swap::Keep | Swap::AskLayer => Self::AsDrawn,
            Swap::DrawAgainNext => Self::Owed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_link_frame_never_asks_the_layer_for_a_drawable() {
        assert_ne!(DrawableSource::Link.wrong_format(), Swap::AskLayer);
        for has_glass in [false, true] {
            for framebuffer_only in [false, true] {
                assert_ne!(
                    DrawableSource::Link.for_glass(has_glass, framebuffer_only),
                    Swap::AskLayer
                );
            }
        }
    }

    #[test]
    fn glass_on_a_framebuffer_only_link_drawable_is_presented_again() {
        let swap = DrawableSource::Link.for_glass(true, true);
        assert_eq!(swap, Swap::DrawAgainNext);
        assert_eq!(Presented::from(swap), Presented::Owed);
        assert_eq!(DrawableSource::Link.for_glass(true, false), Swap::Keep);
        assert_eq!(DrawableSource::Link.for_glass(false, true), Swap::Keep);
    }

    #[test]
    fn without_a_link_the_layer_is_asked_again() {
        assert_eq!(DrawableSource::Layer.for_glass(true, true), Swap::AskLayer);
        assert_eq!(DrawableSource::Layer.wrong_format(), Swap::AskLayer);
        assert_eq!(
            Presented::from(DrawableSource::Layer.for_glass(true, true)),
            Presented::AsDrawn
        );
    }
}
