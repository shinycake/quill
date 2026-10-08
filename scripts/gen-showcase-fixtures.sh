#!/bin/bash
# Regenerates the procedural art used by the ready-showcase demo (ImageMagick 7).
set -e
O="$(cd "$(dirname "$0")/.." && pwd)/docs/screenshots/fixtures/showcase"
mkdir -p $O
cd $O
W=1280; H=860

# p1: sunset over layered mountains
magick -size ${W}x${H} gradient:'#1d2b64'-'#f8cdda' \
  \( -size ${W}x${H} radial-gradient:'#ffd27a'-none -geometry +300+250 -gravity center \) -compose screen -composite \
  -fill '#ffe9b0' -draw "circle 640,470 640,350" -blur 0x14 \
  -fill '#3b2a5a' -draw "polygon 0,640 160,500 300,610 470,430 640,600 820,470 980,590 1120,480 1280,600 1280,860 0,860" \
  -fill '#26204a' -draw "polygon 0,720 200,590 380,700 560,560 760,700 960,600 1140,710 1280,640 1280,860 0,860" \
  -fill '#150f2e' -draw "polygon 0,800 180,720 360,790 560,730 760,800 980,730 1280,810 1280,860 0,860" \
  -attenuate 0.08 +noise Gaussian -quality 82 p1.jpg

# p2: night lake with aurora
magick -size ${W}x${H} gradient:'#030b1f'-'#0b3a4a' \
  \( -size ${W}x${H} xc:none -fill 'rgba(60,255,170,0.55)' -draw "polygon 0,300 200,150 420,260 640,90 860,230 1100,120 1280,260 1280,420 1040,330 820,430 600,300 380,420 180,330 0,420" -blur 0x40 \) -compose screen -composite \
  \( -size ${W}x${H} xc:none -fill 'rgba(150,90,255,0.45)' -draw "polygon 100,200 380,120 700,220 1000,100 1280,180 1280,260 1000,220 700,330 380,240 100,300" -blur 0x50 \) -compose screen -composite \
  -fill white -draw "point 100,60" \
  -fill '#02060f' -draw "polygon 0,600 220,560 400,610 620,570 860,620 1060,570 1280,610 1280,860 0,860" \
  -fill '#06192b' -draw "rectangle 0,640 1280,860" -blur 0x1 \
  -quality 82 p2.jpg
# stars
for i in $(seq 1 90); do :; done
magick p2.jpg \( -size ${W}x400 xc:black +noise Random -threshold 99.6% -blur 0x0.6 -alpha off \) -geometry +0+0 -compose screen -composite -quality 82 p2.jpg

# p3: misty forest
magick -size ${W}x${H} gradient:'#cfe8d5'-'#2f6b4f' \
  -fill '#7fb394' -draw "polygon 0,560 80,300 160,560 240,260 340,580 420,320 520,570 620,240 720,590 820,300 920,570 1020,270 1120,580 1200,330 1280,560 1280,860 0,860" -blur 0x4 \
  -fill '#3f8061' -draw "polygon 0,700 120,420 240,700 380,380 520,710 660,400 800,720 940,390 1100,720 1200,430 1280,700 1280,860 0,860" -blur 0x2 \
  -fill '#17402f' -draw "polygon 0,860 100,560 200,860 320,520 440,860 560,540 700,860 840,500 980,860 1100,540 1200,860" \
  \( -size ${W}x${H} gradient:'rgba(255,255,255,0.0)'-'rgba(255,255,255,0.55)' \) -compose screen -composite \
  -quality 82 p3.jpg

# p4: bokeh city lights
magick -size ${W}x${H} gradient:'#14061f'-'#3a0f4a' bokeh_base.png
args=()
colors=('#ff4d8d' '#ffb347' '#5ee1ff' '#9b7bff' '#ffe14d' '#4dffb8' '#ff7a45')
i=0
while [ $i -lt 46 ]; do
  x=$(( (i*397) % 1280 )); y=$(( (i*251) % 860 )); r=$(( 24 + (i*37) % 70 ))
  c=${colors[$((i%7))]}
  args+=( -fill "${c}66" -stroke "${c}cc" -strokewidth 2 -draw "circle $x,$y $((x+r)),$y" )
  i=$((i+1))
done
magick bokeh_base.png "${args[@]}" -blur 0x3 -quality 82 p4.jpg
rm bokeh_base.png

# p5: desert dunes
magick -size ${W}x${H} gradient:'#ffb36b'-'#ffe3b5' \
  -fill '#ffd9a0' -draw "circle 960,260 960,200" -blur 0x6 \
  -fill '#e08a4a' -draw "polygon 0,520 260,430 520,520 800,400 1100,520 1280,470 1280,860 0,860" \
  -fill '#c46a36' -draw "polygon 0,640 300,540 620,650 900,560 1280,680 1280,860 0,860" \
  -fill '#8f4425' -draw "polygon 0,760 340,690 700,780 1000,710 1280,790 1280,860 0,860" \
  -quality 82 p5.jpg

# p6: ocean at dusk
magick -size ${W}x${H} gradient:'#ff9a8b'-'#2b5876' \
  -fill '#ffd9c0' -draw "circle 640,430 640,380" -blur 0x10 \
  -fill 'rgba(20,50,90,0.85)' -draw "rectangle 0,470 1280,860" \
  \( -size ${W}x390 gradient:'rgba(255,190,150,0.7)'-'rgba(10,30,70,0.9)' \) -geometry +0+470 -compose over -composite \
  -quality 82 p6.jpg

# Avatars: gradient discs with abstract shapes.
mk() { # name c1 c2 shape-fill shape
  magick -size 320x320 gradient:"$2"-"$3" -rotate $4 +repage -gravity center -crop 192x192+0+0 +repage \
    -fill "rgba(255,255,255,0.28)" -draw "$5" -fill "rgba(255,255,255,0.14)" -draw "$6" $1.png
}
mk av01 '#ff9a9e' '#fad0c4' 20 "circle 60,60 60,130" "circle 150,150 150,60"
mk av02 '#a18cd1' '#fbc2eb' 70 "polygon 0,192 96,40 192,192" "circle 150,50 150,10"
mk av03 '#84fab0' '#8fd3f4' 110 "circle 100,100 100,170" "polygon 0,0 110,0 0,110"
mk av04 '#f6d365' '#fda085' 30 "circle 140,60 140,140" "polygon 0,192 120,192 0,80"
mk av05 '#4facfe' '#00f2fe' 160 "polygon 192,0 192,192 60,192" "circle 50,130 50,40"
mk av06 '#fa709a' '#fee140' 200 "circle 96,96 96,170" "polygon 0,60 192,130 192,192 0,192"
mk av07 '#667eea' '#764ba2' 45 "circle 140,140 140,60" "circle 40,50 40,120"
mk av08 '#43e97b' '#38f9d7' 120 "polygon 0,192 192,100 192,192" "circle 60,60 60,10"
mk av09 '#f093fb' '#f5576c' 15 "circle 90,110 90,185" "polygon 192,0 192,120 90,0"
mk av10 '#30cfd0' '#330867' 75 "polygon 0,0 192,192 0,192" "circle 150,50 150,120"
mk av11 '#ff758c' '#ff7eb3' 100 "circle 70,70 70,150" "circle 160,140 160,60"
mk av12 '#fddb92' '#d1fdff' 50 "polygon 0,192 192,60 192,192" "circle 60,60 60,20"
mk av13 '#13547a' '#80d0c7' 140 "circle 130,70 130,150" "polygon 0,150 150,192 0,192"
mk av14 '#e96443' '#904e95' 85 "polygon 0,192 96,30 192,192" "circle 40,40 40,100"
mk av15 '#0ba360' '#3cba92' 25 "circle 96,96 96,30" "polygon 192,192 192,60 60,192"
mk av16 '#7f7fd5' '#91eae4' 155 "circle 60,140 60,50" "polygon 0,0 192,0 192,90"

# Album cover for the audio track.
magick -size 700x700 gradient:"#f12711"-"#f5af19" -rotate 35 +repage -gravity center -crop 400x400+0+0 +repage \
  -fill 'rgba(0,0,0,0.25)' -draw "circle 200,200 200,60" -fill 'rgba(255,255,255,0.25)' -draw "circle 200,200 200,150" cover1.png

# Shrink photos for repo weight.
for f in p1 p2 p3 p4 p5 p6; do magick $f.jpg -resize 1100x -quality 80 $f.jpg; done
ls -la
