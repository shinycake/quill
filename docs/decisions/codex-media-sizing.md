# Media sizing

Photos, GIFs and videos rendered in a fixed 240×140 box with `ObjectFit::Cover`, which cropped every real picture to a strip. Placeholders were a different size (240×88), so rows changed height when the file arrived. `media_frame(width, height)` now fits the media's own aspect ratio into at most 360×400 (short side at least 120). Image and placeholder share that frame, so the row's height never changes on download. Photos prefer the largest downloaded size over the ≤320px "m" thumbnail, because a 360pt frame on Retina needs ~720px.

Media-led bubbles (photo/GIF/video without a reply or forward header) use a 4px inset instead of text padding. Captions get their own padding. Media with nothing beneath it shows its time on a translucent pill over the picture. The helpers no longer add a top margin; callers that stack media under other content add it.

Out of scope: blurred minithumbnail placeholders and download progress rings.
