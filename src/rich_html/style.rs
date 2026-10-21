//! The stylesheet and slideshow script a saved page carries inline. Both
//! are Quill's own, written for the tags and classes tdesktop's export
//! uses, so the page reads like a rich message: a single centred column,
//! rounded media, quotes with an accent rule, light and dark schemes.

pub(super) const STYLE: &str = "\
:root{color-scheme:light dark;--bg:#fff;--text:#111;--muted:#6d7378;--accent:#1d82d2;--line:#e1e4e7;--soft:#f3f5f6;--code:#b4245a;--code-bg:#f7eef2;--pre:#f4f4f5}\n\
@media (prefers-color-scheme:dark){:root{--bg:#181e24;--text:#f2f4f5;--muted:#949ba2;--accent:#5cb7f5;--line:#2d343c;--soft:#2a3139;--code:#f58bb0;--code-bg:#2a3139;--pre:#262c33}\
.math-image,img.math-inline{filter:invert(1)}}\n\
*{box-sizing:border-box}\n\
body{margin:0;padding:0 16px 48px;background:var(--bg);color:var(--text);font:16px/1.5 system-ui,-apple-system,'Segoe UI',Roboto,'Helvetica Neue',Arial,sans-serif}\n\
article{max-width:704px;margin:0 auto;padding-top:24px;overflow-wrap:anywhere}\n\
a{color:var(--accent);text-decoration:none}\n\
a:hover{text-decoration:underline}\n\
h1,h2,h3,h4,h5,h6{font-weight:700;line-height:1.35;margin:14px 0 6px}\n\
h1{font-size:2.2em}h2{font-size:1.95em}h3{font-size:1.65em}h4{font-size:1.4em}h5{font-size:1.25em}h6{font-size:1.1em}\n\
p{margin:0 0 12px}\n\
.byline,footer{display:block;color:var(--muted);margin:0 0 12px}\n\
hr{border:0;border-top:1px solid var(--line);margin:24px 0}\n\
pre{white-space:pre-wrap}\n\
code,tg-math{font-family:ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;font-size:.9em;color:var(--code);background:var(--code-bg);border-radius:4px;padding:1px 4px}\n\
pre.code{margin:0 0 12px;padding:12px 14px;background:var(--pre);border:1px solid var(--line);border-radius:8px;overflow-x:auto}\n\
pre.code>code{color:inherit;background:none;padding:0}\n\
mark{background:rgba(255,200,0,.35);color:inherit;border-radius:3px;padding:0 2px}\n\
blockquote{margin:0 0 12px;padding:4px 12px;border-inline-start:3px solid var(--accent)}\n\
blockquote.thinking{border-inline-start-color:var(--line);color:var(--muted)}\n\
aside{display:block;margin:0 0 12px;text-align:center;font-style:italic}\n\
cite{display:block;margin-top:4px;color:var(--muted);font-style:normal}\n\
figure{margin:0 0 16px}\n\
figure img,figure video{display:block;max-width:100%;height:auto;border-radius:8px}\n\
figcaption{margin-top:6px;color:var(--muted)}\n\
tg-spoiler{background:rgba(128,128,128,.7);color:transparent;border-radius:4px;transition:.2s}\n\
tg-spoiler *{color:transparent}\n\
tg-spoiler img{visibility:hidden}\n\
tg-spoiler:hover{background:rgba(128,128,128,.12);color:inherit}\n\
tg-spoiler:hover *{color:inherit}\n\
tg-spoiler:hover img{visibility:visible}\n\
img[tg-spoiler],video[tg-spoiler]{filter:blur(40px)}\n\
img[tg-spoiler]:hover,video[tg-spoiler]:hover{filter:none}\n\
tg-collage,tg-slideshow{display:block;margin:0 0 16px}\n\
.collage-box{position:relative;overflow:hidden;border-radius:8px}\n\
.collage-item{position:absolute;overflow:hidden;padding:1px}\n\
.collage-item img,.collage-item video,.slide img,.slide video{width:100%;height:100%;border-radius:0}\n\
.collage-item img,.collage-item video{object-fit:cover}\n\
.slideshow-box{position:relative;direction:ltr;overflow:hidden;border-radius:8px;background:#000}\n\
.slideshow-track{display:flex;height:100%;transition:transform .25s ease}\n\
.slide{flex:0 0 100%;display:flex;align-items:center;justify-content:center;overflow:hidden}\n\
.slide img,.slide video{object-fit:contain}\n\
.slide .media-missing{width:100%}\n\
.slideshow-prev,.slideshow-next{position:absolute;top:50%;transform:translateY(-50%);width:36px;height:36px;padding:0;border:0;border-radius:50%;background:#fff;color:#222;font-size:16px;line-height:36px;cursor:pointer;opacity:.85}\n\
.slideshow-prev{left:12px}.slideshow-next{right:12px}\n\
.slideshow-prev:hover,.slideshow-next:hover{opacity:1}\n\
.slideshow-dots{position:absolute;left:0;right:0;bottom:10px;display:flex;justify-content:center;gap:6px}\n\
.slideshow-dot{width:8px;height:8px;padding:0;border:0;border-radius:50%;background:rgba(255,255,255,.5);cursor:pointer}\n\
.slideshow-dot.active{background:#fff}\n\
.table-wrap{overflow-x:auto}\n\
.table-title{font-weight:600;margin-bottom:6px}\n\
table{border-collapse:collapse;margin:0 0 4px}\n\
td,th{padding:6px 10px;text-align:start}\n\
table.bordered td,table.bordered th{border:1px solid var(--line)}\n\
table.striped tr:nth-child(even){background:var(--soft)}\n\
table.compact td,table.compact th{padding-top:3px;padding-bottom:3px}\n\
details{margin:0 0 12px;padding:8px 12px;border:1px solid var(--line);border-radius:8px}\n\
details>summary{cursor:pointer;font-weight:600}\n\
.audio-meta{margin-top:4px;color:var(--muted)}\n\
.media-missing{padding:24px;border-radius:8px;background:var(--soft);color:var(--muted);text-align:center}\n\
tg-math-block{display:block;margin:0 0 12px;padding:12px;border-radius:8px;background:var(--pre);text-align:center;white-space:pre-wrap;overflow-x:auto}\n\
tg-map{display:none}\n\
tg-time{border-bottom:1px dotted currentColor}\n\
.channel,.related-card,.embed-post{display:block;margin:0 0 12px;padding:10px 14px;border:1px solid var(--line);border-radius:8px}\n\
.related-title,.related-name,.embed-post-header{font-weight:600}\n\
.related-title{margin-bottom:6px}\n\
.embed-post-header{margin-bottom:8px}\n\
.related-desc,.related-meta{color:var(--muted)}\n\
.related-card::after{content:'';display:block;clear:both}\n\
.related-card img{float:inline-end;width:64px;height:64px;margin-inline-start:12px;border-radius:6px;object-fit:cover}\n\
.embed-post-photo{float:inline-start;width:40px;height:40px;margin-inline-end:10px;border-radius:50%;object-fit:cover}\n\
li{margin:0 0 6px}\n\
li:has(>input[type=checkbox]){list-style:none;margin-inline-start:-1.2em}\n\
li>input[type=checkbox]{margin:0 6px 0 0;vertical-align:-2px}\n\
img.inline-image{display:inline-block;vertical-align:middle;width:auto;max-height:1.6em;border-radius:0}\n\
";

/// Slideshow arrows and dots: show one slide at a time, and play only the
/// visible slide's autoplaying video.
pub(super) const SCRIPT: &str = "\
(function(){\n\
  document.querySelectorAll('tg-slideshow').forEach(function(show){\n\
    var track=show.querySelector('.slideshow-track');\n\
    if(!track||!track.children.length){return;}\n\
    var slides=Array.prototype.slice.call(track.children);\n\
    var dots=Array.prototype.slice.call(show.querySelectorAll('.slideshow-dot'));\n\
    var current=0;\n\
    function go(to){\n\
      var n=slides.length;\n\
      current=((to%n)+n)%n;\n\
      track.style.transform='translateX('+(-100*current)+'%)';\n\
      dots.forEach(function(dot,i){dot.classList.toggle('active',i===current);});\n\
      slides.forEach(function(slide,i){\n\
        slide.querySelectorAll('video').forEach(function(video){\n\
          if(i!==current){if(!video.paused){video.pause();}}\n\
          else if(video.autoplay&&video.paused){var p=video.play();if(p&&p.catch){p.catch(function(){});}}\n\
        });\n\
      });\n\
    }\n\
    var prev=show.querySelector('.slideshow-prev');\n\
    var next=show.querySelector('.slideshow-next');\n\
    if(prev){prev.addEventListener('click',function(){go(current-1);});}\n\
    if(next){next.addEventListener('click',function(){go(current+1);});}\n\
    dots.forEach(function(dot,i){dot.addEventListener('click',function(){go(i);});});\n\
    go(0);\n\
  });\n\
})();\n\
";
