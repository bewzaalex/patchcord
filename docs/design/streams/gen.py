import math
O=[]
def a(s): O.append(s)
ORANGE='#b45a2e'; INK='#f3efe6'; PRINT='#d5d7d9'
def pol(cx,cy,ang,r):
    t=math.radians(ang); return cx+r*math.sin(t), cy-r*math.cos(t)
def knob(cx,cy,r,ang,scale=True,red=True,pointer='#ffb02e'):
    if scale:
        for i in range(13):
            an=-135+270*i/12; x1,y1=pol(cx,cy,an,r+3.5); x2,y2=pol(cx,cy,an,r+(7 if i%4==0 else 5.5))
            a(f'<path d="M{x1:.1f},{y1:.1f} L{x2:.1f},{y2:.1f}" stroke="{PRINT}" stroke-width=".9" opacity=".6"/>')
        if red:
            s=-135+270/1.53; x1,y1=pol(cx,cy,s,r+5.5); x2,y2=pol(cx,cy,135,r+5.5)
            a(f'<path d="M{x1:.1f},{y1:.1f} A{r+5.5},{r+5.5} 0 0 1 {x2:.1f},{y2:.1f}" fill="none" stroke="#d2402f" stroke-width="2" opacity=".85"/>')
    a(f'<circle cx="{cx}" cy="{cy+1.5}" r="{r+.5}" fill="#111" opacity=".5"/><circle cx="{cx}" cy="{cy}" r="{r}" fill="url(#gVol)" stroke="#0c0c0d"/>')
    x1,y1=pol(cx,cy,ang,r*.3); x2,y2=pol(cx,cy,ang,r*.85)
    a(f'<path d="M{x1:.1f},{y1:.1f} L{x2:.1f},{y2:.1f}" stroke="{pointer}" stroke-width="{2.2 if r>9 else 1.6}" stroke-linecap="round"/>')
def balknob(cx,cy,r,b):
    for i in range(-4,5):
        an=i*33.75; x1,y1=pol(cx,cy,an,r+2.5); x2,y2=pol(cx,cy,an,r+(6 if i==0 else 4))
        a(f'<path d="M{x1:.1f},{y1:.1f} L{x2:.1f},{y2:.1f}" stroke="{PRINT}" stroke-width="{1.4 if i==0 else .9}" opacity="{1 if i==0 else .6}"/>')
    knob(cx,cy,r,b*135,scale=False)
    a(f'<text x="{cx-r-5}" y="{cy+r+8}" font-size="6.5" fill="{PRINT}" text-anchor="middle">L</text><text x="{cx+r+5}" y="{cy+r+8}" font-size="6.5" fill="{PRINT}" text-anchor="middle">R</text>')
def led(cx,cy,on,col='#ff3b2f',r=2.6):
    a(f'<circle cx="{cx}" cy="{cy}" r="{r}" fill="{col if on else "#3a1411"}" stroke="rgba(0,0,0,.6)" stroke-width=".6"/>')
def key(x,y,w,h,text,ledon=None,ledcol='#ff3b2f',fs=7.2):
    a(f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="2.5" fill="#1b1c1e" stroke="rgba(255,255,255,.18)"/>')
    tx=x+w/2
    if ledon is not None: led(x+7,y+h/2,ledon,ledcol,2.3); tx+=4
    a(f'<text x="{tx}" y="{y+h/2+2.6}" font-size="{fs}" fill="#e6e7e8" text-anchor="middle" letter-spacing=".6">{text}</text>')
def meter(x,y,cols,n,lit,w=4,h=3.2,step=4.4,colgap=5.5):
    for c in range(cols):
        for i in range(n):
            col='#ff3b2f' if i>=n-1 else '#ffb02e' if i>=n-2 else '#4cff6a'
            on=i<lit[c]
            a(f'<rect x="{x+c*colgap}" y="{y-i*step}" width="{w}" height="{h}" rx=".6" fill="{col if on else "rgba(0,0,0,.5)"}"/>')
def jack(cx,cy,stereo=True,color=ORANGE,r=1.0):
    pts=' '.join(f'{cx+15*r*math.cos(math.radians(k*60+30)):.1f},{cy+15*r*math.sin(math.radians(k*60+30)):.1f}' for k in range(6))
    a(f'<circle cx="{cx+1.2}" cy="{cy+2.4}" r="{16.5*r}" fill="#000" opacity=".35"/><polygon points="{pts}" fill="url(#gNut)" stroke="#43454a" stroke-width=".8"/>')
    a(f'<circle cx="{cx}" cy="{cy}" r="{11.4*r}" fill="url(#gWasher)"/>')
    if stereo:
        a(f'<circle cx="{cx}" cy="{cy}" r="{10.7*r}" fill="none" stroke="{color}" stroke-width="1"/><circle cx="{cx}" cy="{cy}" r="{9.3*r}" fill="none" stroke="{color}" stroke-width="1"/>')
    a(f'<circle cx="{cx}" cy="{cy}" r="{8.4*r}" fill="url(#gChrome)" stroke="#2a2b2e" stroke-width=".8"/><circle cx="{cx}" cy="{cy}" r="{5.3*r}" fill="url(#gHole)"/>')
def frame(x,y,w,h,label,band=ORANGE,tag='OUT',right=None):
    a(f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="6" fill="rgba(0,0,0,.15)" stroke="rgba(0,0,0,.3)"/>')
    a(f'<path d="M{x},{y+18} V{y+6} a6,6 0 0 1 6,-6 H{x+w-6} a6,6 0 0 1 6,6 V{y+18} Z" fill="{band}"/>')
    a(f'<rect x="{x+.6}" y="{y+.6}" width="{w-1.2}" height="{h-1.2}" rx="5.6" fill="none" stroke="{band}" stroke-width="1.2" opacity=".9"/>')
    a(f'<text x="{x+8}" y="{y+13}" font-size="8.5" fill="{INK}" letter-spacing=".8" font-weight="bold">{label}</text>')
def device(x,y,w,title='FIREFOX',model='APP · PLAYBACK',h=128,lcd=('Firefox','RUNNING   3 LINKS','3 STREAMS · STEREO')):
    a(f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="12" fill="#26272a"/><rect x="{x}" y="{y}" width="{w}" height="{h}" rx="12" fill="url(#gSheen)"/>')
    a(f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="12" fill="none" stroke="rgba(0,0,0,.65)"/>')
    for sx,sy in [(x+11,y+11),(x+w-11,y+11),(x+11,y+h-11),(x+w-11,y+h-11)]:
        a(f'<circle cx="{sx}" cy="{sy}" r="5" fill="url(#gChrome)" stroke="#35363a" stroke-width=".7"/><path d="M{sx-3},{sy} H{sx+3} M{sx},{sy-3} V{sy+3}" stroke="#36373b" stroke-width="1.3" stroke-linecap="round"/>')
    a(f'<text x="{x+16}" y="{y+33}" font-size="14" font-weight="900" fill="{PRINT}" font-family="Noto Sans">{title}</text>')
    a(f'<text x="{x+16}" y="{y+46}" font-size="7.8" fill="#9a9da1" letter-spacing="1">{model}</text>')
    a(f'<rect x="{x+14.5}" y="{y+51}" width="157" height="63" rx="4" fill="#111214"/><rect x="{x+17.5}" y="{y+54}" width="151" height="57" rx="2" fill="url(#gLcd)"/>')
    a(f'<g font-family="Noto Sans Mono" font-size="9.5" fill="#3a2408"><text x="{x+24}" y="{y+68}" font-weight="bold">{lcd[0]}</text><text x="{x+24}" y="{y+84}">{lcd[1]}</text><text x="{x+24}" y="{y+100}">{lcd[2]}</text></g>')
def master(x,y,mw=100):
    # VOLUME · BAL · MUTE for the whole app, inside the frame after a groove
    a(f'<path d="M{x},{y+22} V{y+84}" stroke="rgba(0,0,0,.45)"/><path d="M{x+1},{y+22} V{y+84}" stroke="rgba(255,255,255,.1)"/>')
    a(f'<text x="{x+mw/2}" y="{y+13}" font-size="8.5" fill="{INK}" text-anchor="middle" letter-spacing=".8" font-weight="bold">VOLUME · BAL</text>')
    knob(x+33,y+42,14,volang(.9)); balknob(x+76,y+38,9,0)
    key(x+7,y+71,mw-14,14,'MUTE',False)
def volang(v): return -135+270*v/1.53
def master_split(x,y,join,mw=100):
    a(f'<path d="M{x},{y+22} V{y+84}" stroke="rgba(0,0,0,.45)"/><path d="M{x+1},{y+22} V{y+84}" stroke="rgba(255,255,255,.1)"/>')
    a(f'<text x="{x+mw/2}" y="{y+13}" font-size="8.5" fill="{INK}" text-anchor="middle" letter-spacing=".8" font-weight="bold">VOLUME · BAL</text>')
    knob(x+33,y+42,14,volang(.9)); balknob(x+76,y+38,9,0)
    key(x+7,y+71,38,14,'JOIN' if join else 'SPLIT',fs=6.6); key(x+49,y+71,mw-56,14,'MUTE',False)
def title(x,y,t,sub):
    a(f'<text x="{x}" y="{y}" font-size="17" font-weight="bold" fill="#ffb44e" font-family="Noto Sans">{t}</text>')
    a(f'<text x="{x}" y="{y+18}" font-size="11" fill="#b9bbbe" font-family="Noto Sans">{sub}</text>')
streams=[(.85,0,[3,4],'YouTube'),(.0,0,[0,0],'Spotify'),(.6,-.4,[2,1],'Meet')]

W=1560; H=3330
a(f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}" font-family="Noto Sans" font-weight="bold">')
a('''<defs><radialGradient id="gVol" cx=".38" cy=".3" r=".8"><stop offset="0" stop-color="#5a5b5f"/><stop offset="1" stop-color="#151618"/></radialGradient>
<linearGradient id="gSheen" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#fff" stop-opacity=".16"/><stop offset=".35" stop-color="#fff" stop-opacity="0"/><stop offset="1" stop-color="#000" stop-opacity=".24"/></linearGradient>
<linearGradient id="gNut" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#f6f7f8"/><stop offset=".45" stop-color="#b6b9bd"/><stop offset="1" stop-color="#55585d"/></linearGradient>
<radialGradient id="gWasher" cx=".4" cy=".35" r=".75"><stop offset="0" stop-color="#5e6166"/><stop offset="1" stop-color="#141517"/></radialGradient>
<radialGradient id="gChrome" cx=".35" cy=".3" r=".85"><stop offset="0" stop-color="#fff"/><stop offset=".4" stop-color="#b9bcc1"/><stop offset=".75" stop-color="#65686d"/><stop offset="1" stop-color="#a6a9ad"/></radialGradient>
<radialGradient id="gHole" cx=".5" cy=".62" r=".6"><stop offset="0" stop-color="#000"/><stop offset=".7" stop-color="#0a0a0b"/><stop offset="1" stop-color="#3c3d40"/></radialGradient>
<linearGradient id="gLcd" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#f7bb62"/><stop offset="1" stop-color="#e38c2a"/></linearGradient>
<linearGradient id="gDrawer" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#1d1e21"/><stop offset="1" stop-color="#141517"/></linearGradient></defs>''')
a(f'<rect width="{W}" height="{H}" fill="#1e1f22"/>')

# ---------- 1: everything on the face
X,Y=30,30
title(X,Y,'1 · ON THE FACE','each tab: meter, mute, trim and balance under its jack; the frame: VOLUME · BAL for all')
dy=Y+36; device(X,dy,760)
fx=X+200; frame(fx,dy+18,440,92,'OUTPUT')
for i,(v,b,lv,n) in enumerate(streams):
    cx=fx+44+i*92
    jack(cx,dy+50); meter(cx+19,dy+64,2,5,lv)
    a(f'<text x="{cx}" y="{dy+77}" font-size="9" fill="{PRINT}" text-anchor="middle">{i+1}</text>')
    led(cx-24,dy+95,False if i!=1 else True,r=2.5); a(f'<circle cx="{cx-24}" cy="{dy+95}" r="5.2" fill="none" stroke="rgba(255,255,255,.18)"/>')
    knob(cx-4,dy+95,6.5,volang(v),scale=False); knob(cx+17,dy+95,5.5,b*135,scale=False)
a(f'<text x="{fx+290}" y="{dy+103}" font-size="6" fill="{PRINT}">VOL BAL</text>')
key(fx+290,dy+72,40,12,'SPLIT')
master(fx+340,dy+18)
a(f'<text x="{X+780}" y="{dy+40}" font-size="11" fill="#9a9da1" font-weight="normal">+ all at a glance, nothing to open</text>')
a(f'<text x="{X+780}" y="{dy+58}" font-size="11" fill="#9a9da1" font-weight="normal">− dense: 3 tiny controls per tab; ~92 px per tab</text>')
a(f'<text x="{X+780}" y="{dy+76}" font-size="11" fill="#9a9da1" font-weight="normal">− no room while split (L above R)</text>')

# ---------- 2: drawer with knobs
Y=260
title(X,Y,'2 · DRAWER, KNOBS','the frame: VOLUME · BAL for all; STREAMS ▾ slides out a strip per tab: stereo meter, VOLUME, BAL, MUTE, CUE')
dy=Y+36; device(X,dy,640)
fx=X+200; frame(fx,dy+18,320,92,'OUTPUT')
for i in range(3): jack(fx+40+i*52,dy+50); a(f'<text x="{fx+40+i*52}" y="{dy+77}" font-size="9" fill="{PRINT}" text-anchor="middle">{i+1}</text>')
key(fx+8,dy+92,64,13,'STREAMS ▴'); key(fx+174,dy+92,40,12,'SPLIT')
master(fx+220,dy+18)
dx,dyy=X+14,dy+126
a(f'<rect x="{dx}" y="{dyy}" width="560" height="150" rx="8" fill="url(#gDrawer)" stroke="#000"/>')
for i,(v,b,lv,n) in enumerate(streams):
    sx=dx+12+i*180
    a(f'<rect x="{sx}" y="{dyy+10}" width="170" height="130" rx="5" fill="rgba(255,255,255,.04)" stroke="rgba(0,0,0,.6)"/>')
    a(f'<rect x="{sx+8}" y="{dyy+18}" width="18" height="14" rx="3" fill="{ORANGE}"/><text x="{sx+17}" y="{dyy+28.5}" font-size="10" fill="{INK}" text-anchor="middle">{i+1}</text>')
    a(f'<text x="{sx+32}" y="{dyy+29}" font-size="9" fill="#d5d7d9" font-weight="normal">{n}</text>')
    meter(sx+12,dyy+110,2,8,[l*2 for l in lv],w=5,h=4,step=7,colgap=7)
    knob(sx+60,dyy+78,13,volang(v)); a(f'<text x="{sx+60}" y="{dyy+108}" font-size="7" fill="{PRINT}" text-anchor="middle">VOLUME</text>')
    balknob(sx+120,dyy+74,9,b); a(f'<text x="{sx+120}" y="{dyy+108}" font-size="7" fill="{PRINT}" text-anchor="middle">BAL</text>')
    key(sx+36,dyy+116,58,15,'MUTE',i==1); key(sx+100,dyy+116,58,15,'CUE',False,'#5fd4ff')
a(f'<text x="{X+660}" y="{dy+40}" font-size="11" fill="#9a9da1" font-weight="normal">+ calm face; big, clear controls; names shown</text>')
a(f'<text x="{X+660}" y="{dy+58}" font-size="11" fill="#9a9da1" font-weight="normal">+ any number of tabs; works while split</text>')
a(f'<text x="{X+660}" y="{dy+76}" font-size="11" fill="#9a9da1" font-weight="normal">− one click away</text>')

# ---------- 3: drawer, console
Y=620
title(X,Y,'3 · DRAWER, MIXING CONSOLE','the drawer as a console: per tab BAL (pan) on top, a fader with its meter, MUTE and CUE at the bottom')
dy=Y+36; device(X,dy,640)
fx=X+200; frame(fx,dy+18,320,92,'OUTPUT')
for i in range(3): jack(fx+40+i*52,dy+50); a(f'<text x="{fx+40+i*52}" y="{dy+77}" font-size="9" fill="{PRINT}" text-anchor="middle">{i+1}</text>')
key(fx+8,dy+92,64,13,'STREAMS ▴'); key(fx+174,dy+92,40,12,'SPLIT')
master(fx+220,dy+18)
dx,dyy=X+14,dy+126
a(f'<rect x="{dx}" y="{dyy}" width="330" height="250" rx="8" fill="url(#gDrawer)" stroke="#000"/>')
for i,(v,b,lv,n) in enumerate(streams):
    sx=dx+12+i*104
    a(f'<rect x="{sx}" y="{dyy+10}" width="96" height="230" rx="5" fill="rgba(255,255,255,.04)" stroke="rgba(0,0,0,.6)"/>')
    a(f'<rect x="{sx+39}" y="{dyy+18}" width="18" height="14" rx="3" fill="{ORANGE}"/><text x="{sx+48}" y="{dyy+28.5}" font-size="10" fill="{INK}" text-anchor="middle">{i+1}</text>')
    a(f'<text x="{sx+48}" y="{dyy+44}" font-size="8" fill="#d5d7d9" font-weight="normal" text-anchor="middle">{n}</text>')
    balknob(sx+48,dyy+66,9,b)
    # fader slot, its cap at the volume, a meter beside
    top,bot=dyy+92,dyy+186
    a(f'<rect x="{sx+44}" y="{top}" width="8" height="{bot-top}" rx="4" fill="#0a0a0b" stroke="rgba(255,255,255,.12)"/>')
    for k in range(7):
        ty=top+k*(bot-top)/6; a(f'<path d="M{sx+34},{ty:.1f} H{sx+40}" stroke="{PRINT}" stroke-width=".8" opacity=".6"/>')
    y100=bot-(bot-top)/1.53; a(f'<path d="M{sx+30},{y100:.1f} H{sx+40}" stroke="{PRINT}" stroke-width="1.6"/>')
    cy=bot-(bot-top)*v/1.53
    a(f'<rect x="{sx+36}" y="{cy-7}" width="24" height="14" rx="2" fill="url(#gVol)" stroke="#0c0c0d"/><path d="M{sx+38},{cy} H{sx+58}" stroke="#ffb02e" stroke-width="1.6"/>')
    meter(sx+68,bot-4,2,9,[round(l*2.2) for l in lv],w=5,h=7,step=10,colgap=7)
    key(sx+8,dyy+196,80,15,'MUTE',i==1); key(sx+8,dyy+216,80,15,'CUE',False,'#5fd4ff')
a(f'<text x="{X+660}" y="{dy+40}" font-size="11" fill="#9a9da1" font-weight="normal">+ reads like a real mixer: faders show levels at a glance</text>')
a(f'<text x="{X+660}" y="{dy+58}" font-size="11" fill="#9a9da1" font-weight="normal">+ narrow strips: many tabs fit</text>')
a(f'<text x="{X+660}" y="{dy+76}" font-size="11" fill="#9a9da1" font-weight="normal">− tall drawer (covers devices below); faders new to Patchcord</text>')

# ---------- 4: hybrid
Y=1100
title(X,Y,'4 · HYBRID','on the face: each tab\'s meter and MUTE (what you reach for in a call); in the drawer: VOLUME and BAL per tab')
dy=Y+36; device(X,dy,700)
fx=X+200; frame(fx,dy+18,380,92,'OUTPUT')
for i,(v,b,lv,n) in enumerate(streams):
    cx=fx+44+i*64
    jack(cx,dy+50); meter(cx+19,dy+64,2,5,lv)
    a(f'<text x="{cx}" y="{dy+77}" font-size="9" fill="{PRINT}" text-anchor="middle">{i+1}</text>')
    a(f'<circle cx="{cx}" cy="{dy+95}" r="5.2" fill="#1b1c1e" stroke="rgba(255,255,255,.18)"/>'); led(cx,dy+95,i==1,r=2.4)
key(fx+200,dy+92,64,13,'STREAMS ▾'); key(fx+232,dy+72,40,12,'SPLIT')
master(fx+280,dy+18)
a(f'<text x="{X+720}" y="{dy+40}" font-size="11" fill="#9a9da1" font-weight="normal">+ mute and activity per tab always visible</text>')
a(f'<text x="{X+720}" y="{dy+58}" font-size="11" fill="#9a9da1" font-weight="normal">+ fine-tuning (volume, balance) in the drawer, as in 2</text>')
a(f'<text x="{X+720}" y="{dy+76}" font-size="11" fill="#9a9da1" font-weight="normal">− two places to look</text>')

# ---------- 5: two units high
Y=1360
title(X,Y,'5 · TWO UNITS HIGH','the device grows to 2U while the app has several streams: the second unit is a channel strip under each tab\'s jack')
dy=Y+36
w5=760; h5=256
device(X,dy,w5,h=h5)
# the seam between the two units, pressed into the metal
a(f'<path d="M{X+8},{dy+128} H{X+w5-8}" stroke="rgba(0,0,0,.55)" stroke-width="1.2"/><path d="M{X+8},{dy+129.2} H{X+w5-8}" stroke="rgba(255,255,255,.08)"/>')
fx=X+200; frame(fx,dy+18,430,92,'OUTPUT')
cols=[fx+60+i*120 for i in range(3)]
for i,cx in enumerate(cols):
    jack(cx,dy+56); a(f'<text x="{cx}" y="{dy+84}" font-size="9" fill="{PRINT}" text-anchor="middle">{i+1}</text>')
key(fx+330-46,dy+92,40,12,'SPLIT')
master(fx+330,dy+18)
# the second unit: the streams, a strip under each jack, in one frame
frame(fx,dy+140,430,104,'STREAMS',tag=None)
for i,(v,b,lv,n) in enumerate(streams):
    cx=cols[i]
    if i: a(f'<path d="M{cx-60},{dy+162} V{dy+236}" stroke="rgba(0,0,0,.45)"/><path d="M{cx-59},{dy+162} V{dy+236}" stroke="rgba(255,255,255,.08)"/>')
    a(f'<text x="{cx}" y="{dy+171}" font-size="8" fill="{PRINT}" font-weight="normal" text-anchor="middle">{i+1} · {n}</text>')
    meter(cx-50,dy+210,2,6,[round(l*1.5) for l in lv],w=4.5,h=3.6,step=5.6,colgap=6.5)
    knob(cx-10,dy+194,10,volang(v))
    balknob(cx+30,dy+192,7,b)
    key(cx-50,dy+222,46,13,'MUTE',i==1,fs=6.5); key(cx-0,dy+222,46,13,'CUE',False,'#5fd4ff',fs=6.5)
a(f'<text x="{X+780}" y="{dy+40}" font-size="11" fill="#9a9da1" font-weight="normal">+ everything visible, full-size controls, names shown</text>')
a(f'<text x="{X+780}" y="{dy+58}" font-size="11" fill="#9a9da1" font-weight="normal">+ each strip right under its jack: what is what is plain</text>')
a(f'<text x="{X+780}" y="{dy+76}" font-size="11" fill="#9a9da1" font-weight="normal">+ fits the rack: a 2U unit; nothing slides over other devices</text>')
a(f'<text x="{X+780}" y="{dy+94}" font-size="11" fill="#9a9da1" font-weight="normal">− twice the height on the desk; the OUTPUT frame spreads to match the strips</text>')
a(f'<text x="{X+780}" y="{dy+112}" font-size="11" fill="#9a9da1" font-weight="normal">− many tabs make it wide (could fold to a drawer past ~6)</text>')
# ---------- 6: two units high, a full frame per stream (and the app's other groups)
Y=1720
title(X,Y,'6 · TWO UNITS, A FRAME PER STREAM','each stream a whole frame as today (its jacks, VOLUME or GAIN, MUTE, meter), in rows; its INPUT and any other group take places too')
dy=Y+36
w6=860; h6=256
device(X,dy,w6,h=h6,title='TEAMS FOR LINUX',model='APP · RECORD &amp; PLAY',lcd=('Teams for Linux','RUNNING   6 LINKS','6 STREAMS · MONO'))
a(f'<path d="M{X+8},{dy+128} H{X+w6-8}" stroke="rgba(0,0,0,.55)" stroke-width="1.2"/><path d="M{X+8},{dy+129.2} H{X+w6-8}" stroke="rgba(255,255,255,.08)"/>')
OCHRE='#a8862c'
def mono_frame(fx,fy,label,band,tag,knobname,v,muted,lv,cord,cordcol):
    fw=206
    frame(fx,fy,fw,92,label,band=band)
    jw=110
    if tag=='OUT':
        a(f'<rect x="{fx+jw-30}" y="{fy+3.5}" width="23" height="11" rx="2" fill="{INK}"/><text x="{fx+jw-18.5}" y="{fy+12}" font-size="7.5" fill="{band}" text-anchor="middle">OUT</text>')
    else:
        a(f'<rect x="{fx+jw-30}" y="{fy+3.5}" width="23" height="11" rx="2" fill="none" stroke="{INK}" stroke-width=".9"/><text x="{fx+jw-18.5}" y="{fy+12}" font-size="7.5" fill="{INK}" text-anchor="middle">IN</text>')
    jx=fx+32
    a(f'<rect x="{jx-21}" y="{fy+44-21}" width="{48+42}" height="42" rx="21" fill="rgba(0,0,0,.08)" stroke="{PRINT}" stroke-width=".9" opacity=".5"/>')
    for k in (0,48):
        jack(jx+k,fy+44,stereo=False)
        a(f'<circle cx="{jx+k}" cy="{fy+44}" r="10.1" fill="none" stroke="{band}" stroke-width="1.3"/>')
    a(f'<circle cx="{jx+59.5}" cy="{fy+32.5}" r="5.8" fill="#1b1c1e" stroke="#f2f2f2" stroke-opacity=".35" stroke-width=".8"/><path d="M{jx+56.5},{fy+32.5} h6 M{jx+59.5},{fy+29.5} v6" stroke="#f2f2f2" stroke-width="1.6" stroke-linecap="round"/>')
    if cord:
        a(f'<path d="M{jx},{fy+44} C{jx+12},{fy+90} {jx+50},{fy+110} {jx+80},{fy+160}" stroke="rgba(0,0,0,.45)" stroke-width="9" fill="none" stroke-linecap="round"/><path d="M{jx},{fy+44} C{jx+12},{fy+90} {jx+50},{fy+110} {jx+80},{fy+160}" stroke="{cordcol}" stroke-width="6.6" fill="none" stroke-linecap="round"/>')
    a(f'<text x="{jx+24}" y="{fy+78}" font-size="9" fill="{PRINT}" text-anchor="middle">MONO</text>')
    mx=fx+jw
    a(f'<path d="M{mx},{fy+22} V{fy+84}" stroke="rgba(0,0,0,.45)"/><path d="M{mx+1},{fy+22} V{fy+84}" stroke="rgba(255,255,255,.1)"/>')
    a(f'<text x="{mx+36}" y="{fy+13}" font-size="8.5" fill="{INK}" text-anchor="middle" letter-spacing=".8">{knobname}</text>')
    knob(mx+36,fy+42,13,volang(v)); key(mx+8,fy+71,56,14,'MUTE',muted)
    meter(mx+78,fy+82,1,9,lv[:1],w=4,h=3.6,step=6.6,colgap=5.5)
fx=X+200
mono_frame(fx,dy+18,'INPUT',OCHRE,'IN','GAIN',1,False,[4,4],True,'#d9d4c6')
mono_frame(fx+216,dy+18,'OUTPUT 1',ORANGE,'OUT','VOLUME',1,False,[6,6],True,'#3fae5a')
mono_frame(fx+432,dy+18,'OUTPUT 2',ORANGE,'OUT','VOLUME',.7,False,[2,2],True,'#5b7fe0')
mono_frame(fx,dy+146,'OUTPUT 3',ORANGE,'OUT','VOLUME',1,False,[0,0],True,'#e9c53a')
mono_frame(fx+216,dy+146,'OUTPUT 4',ORANGE,'OUT','VOLUME',1,True,[0,0],True,'#c05bd0')
mono_frame(fx+432,dy+146,'OUTPUT 5',ORANGE,'OUT','VOLUME',1,False,[3,3],True,'#2bb0a8')
a(f'<text x="{X+880}" y="{dy+40}" font-size="11" fill="#9a9da1" font-weight="normal">+ every stream is exactly a normal frame: nothing new to learn</text>')
a(f'<text x="{X+880}" y="{dy+58}" font-size="11" fill="#9a9da1" font-weight="normal">+ its own mult, controls and meter; its INPUT and other groups (a MONITOR...) fill places too</text>')
a(f'<text x="{X+880}" y="{dy+76}" font-size="11" fill="#9a9da1" font-weight="normal">+ one device, two units: no six look-alike devices; rows wrap (3U for more)</text>')
a(f'<text x="{X+880}" y="{dy+94}" font-size="11" fill="#9a9da1" font-weight="normal">− the biggest; grows and shrinks as streams come and go</text>')
a(f'<text x="{X+880}" y="{dy+112}" font-size="11" fill="#9a9da1" font-weight="normal">− no control for the whole app at once (each stream on its own)</text>')
# ---------- 6b: the same with stereo streams (Firefox): each frame its own SPLIT and BAL
Y=2050
dy=Y
a(f'<text x="{X}" y="{dy-8}" font-size="12" fill="#ffb44e" font-family="Noto Sans">6 · with stereo streams (Firefox): each frame its own SPLIT and VOLUME · BAL; tab 3 split into L above R</text>')
w7=760
device(X,dy+6,w7,h=256,lcd=('Firefox','RUNNING   3 LINKS','3 STREAMS · STEREO'))
a(f'<path d="M{X+8},{dy+134} H{X+w7-8}" stroke="rgba(0,0,0,.55)" stroke-width="1.2"/><path d="M{X+8},{dy+135.2} H{X+w7-8}" stroke="rgba(255,255,255,.08)"/>')
def stereo_frame(fx,fy,i,name,v,b,muted,lv,split=False):
    fw=262; jw=136
    frame(fx,fy,fw,92,f'{i} · {name.upper()}')
    a(f'<rect x="{fx+jw-30}" y="{fy+3.5}" width="23" height="11" rx="2" fill="{INK}"/><text x="{fx+jw-18.5}" y="{fy+12}" font-size="7.5" fill="{ORANGE}" text-anchor="middle">OUT</text>')
    if split:
        # L above R, each its own row (a mult each)
        for row,(ly,lab) in enumerate([(fy+36,'L'),(fy+70,'R')]):
            a(f'<text x="{fx+14}" y="{ly+3}" font-size="9" fill="{PRINT}" text-anchor="middle">{lab}</text>')
            jack(fx+40,ly,r=.92); jack(fx+84,ly,r=.92)
    else:
        jx=fx+34
        a(f'<rect x="{jx-21}" y="{fy+44-21}" width="{48+42}" height="42" rx="21" fill="rgba(0,0,0,.08)" stroke="{PRINT}" stroke-width=".9" opacity=".5"/>')
        jack(jx,fy+44); jack(jx+48,fy+44)
        a(f'<text x="{jx+24}" y="{fy+78}" font-size="7.5" fill="{PRINT}" text-anchor="middle" letter-spacing=".6">STEREO</text>')
    mx=fx+jw
    a(f'<path d="M{mx},{fy+22} V{fy+84}" stroke="rgba(0,0,0,.45)"/><path d="M{mx+1},{fy+22} V{fy+84}" stroke="rgba(255,255,255,.1)"/>')
    a(f'<text x="{mx+48}" y="{fy+13}" font-size="8.5" fill="{INK}" text-anchor="middle" letter-spacing=".8">VOLUME · BAL</text>')
    knob(mx+30,fy+42,13,volang(v)); balknob(mx+72,fy+38,8.5,b)
    key(mx+8,fy+71,38,14,'JOIN' if split else 'SPLIT',fs=6.6); key(mx+50,fy+71,42,14,'MUTE',muted)
    meter(mx+100,fy+82,2,9,lv,w=4,h=3.6,step=6.6,colgap=5.5)
fx=X+200
stereo_frame(fx,dy+24,1,'YouTube',.85,0,False,[6,7])
stereo_frame(fx+272,dy+24,2,'Spotify',.6,0,True,[0,0])
stereo_frame(fx,dy+152,3,'Meet',.6,-.4,False,[4,2],split=True)
# ---------- 2, again: levels on the face, controls in the drawer; joined and split
Y=2420
title(X,Y,'2 · DRAWER, LEVELS ON THE FACE','each tab\'s level beside its jack on the face; in the drawer each tab\'s VOLUME, BAL, MUTE; one SPLIT on the face beside MUTE')
def drawer_strips(dx,dyy,split=()):
    a(f'<rect x="{dx}" y="{dyy}" width="490" height="112" rx="8" fill="url(#gDrawer)" stroke="#000"/>')
    for i,(v,b,lv,n) in enumerate(streams):
        sx=dx+12+i*158
        a(f'<rect x="{sx}" y="{dyy+10}" width="150" height="92" rx="5" fill="rgba(255,255,255,.04)" stroke="rgba(0,0,0,.6)"/>')
        a(f'<rect x="{sx+8}" y="{dyy+18}" width="18" height="14" rx="3" fill="{ORANGE}"/><text x="{sx+17}" y="{dyy+28.5}" font-size="10" fill="{INK}" text-anchor="middle">{i+1}</text>')
        a(f'<text x="{sx+32}" y="{dyy+29}" font-size="9" fill="#d5d7d9" font-weight="normal">{n}</text>')
        knob(sx+40,dyy+60,12,volang(v)); balknob(sx+100,dyy+57,8,b)
        key(sx+40,dyy+82,60,14,'MUTE',i==1)
# joined: a stereo meter beside each stereo jack
dy=Y+36
a(f'<text x="{X}" y="{dy-6}" font-size="11" fill="#9a9da1" font-weight="normal">joined</text>')
device(X,dy,660)
fx=X+200; frame(fx,dy+18,340,92,'OUTPUT')
for i,(v,b,lv,n) in enumerate(streams):
    cx=fx+40+i*62
    jack(cx,dy+54); meter(cx+19,dy+70,2,6,lv,w=3.6,h=3.2,step=4.6,colgap=4.6)
    a(f'<text x="{cx}" y="{dy+86}" font-size="9" fill="{PRINT}" text-anchor="middle">{i+1}</text>')
key(fx+8,dy+93,64,13,'STREAMS ▴')
master_split(fx+240,dy+18,False)
drawer_strips(X+14,dy+126)
# split: the face SPLIT splits every tab: a column, L above R, each jack its own meter
dy=Y+300
a(f'<text x="{X}" y="{dy-6}" font-size="11" fill="#9a9da1" font-weight="normal">split</text>')
device(X,dy,660)
fx=X+200; frame(fx,dy+18,340,92,'OUTPUT')
for i,(v,b,lv,n) in enumerate(streams):
    cx=fx+92+i*56
    if False:
        jack(cx,dy+54); meter(cx+19,dy+70,2,6,lv,w=3.6,h=3.2,step=4.6,colgap=4.6)
        a(f'<text x="{cx}" y="{dy+86}" font-size="9" fill="{PRINT}" text-anchor="middle">{i+1}</text>')
    else:
        for row,(ly,lab) in enumerate([(dy+46,'L'),(dy+80,'R')]):
            jack(cx,ly,r=.92); meter(cx+18,ly+9,1,5,[lv[row]],w=3.6,h=3,step=4.2)
            a(f'<text x="{cx-22}" y="{ly+3}" font-size="8" fill="{PRINT}" text-anchor="middle">{lab}</text>')
        a(f'<text x="{cx+27}" y="{dy+100}" font-size="9" fill="{PRINT}" text-anchor="middle">{i+1}</text>')
key(fx+8,dy+93,64,13,'STREAMS ▴')
master_split(fx+240,dy+18,True)
drawer_strips(X+14,dy+126)
a(f'<text x="{X+700}" y="{Y+76}" font-size="11" fill="#9a9da1" font-weight="normal">+ which tab plays is visible on the face, joined or split</text>')
a(f'<text x="{X+700}" y="{Y+94}" font-size="11" fill="#9a9da1" font-weight="normal">+ split, each jack its own meter, L above R</text>')
a(f'<text x="{X+700}" y="{Y+112}" font-size="11" fill="#9a9da1" font-weight="normal">+ the drawer is lower: controls only</text>')
a(f'<text x="{X+700}" y="{Y+130}" font-size="11" fill="#9a9da1" font-weight="normal">· one SPLIT / JOIN beside the face MUTE, for all tabs</text>')
a('</svg>')
open('/home/obevza/.claude/jobs/5043fdb1/tmp/sheet/sheet.svg','w').write('\n'.join(O))
