#!/bin/bash
# usage: cap.sh out.png [display]; crops to the true client area via xwininfo
D=${2:-:93}
WID=$(DISPLAY=$D xdotool search --class openfader | tail -1)
read X Y W H < <(DISPLAY=$D xwininfo -id $WID | awk '/Absolute upper-left X/{x=$4}/Absolute upper-left Y/{y=$4}/Width:/{w=$2}/Height:/{h=$2}END{print x,y,w,h}')
ffmpeg -loglevel error -y -f x11grab -draw_mouse 0 -video_size 1440x1200 -i $D -frames:v 1 -vf "crop=$W:$H:$X:$Y" "$1"
echo "$WID $X $Y $W $H"
