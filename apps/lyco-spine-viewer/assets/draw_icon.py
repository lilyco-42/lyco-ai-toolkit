"""Draw the Spine Viewer app icon: flat skeleton puppet on navy rounded square.

Pure geometry (4x supersampled -> LANCZOS down), so it stays crisp from
1024 down to 16px. Outputs icon-1024.png and icon.ico in this directory.
"""
from PIL import Image, ImageDraw

S = 4  # supersample factor
W = 1024 * S

NAVY = (15, 23, 42, 255)        # #0F172A
TEAL = (34, 211, 238, 255)      # #22D3EE
NODE = (165, 243, 252, 255)     # #A5F3FC joint highlight
GLOW = (34, 211, 238)           # glow tint

img = Image.new("RGBA", (W, W), (0, 0, 0, 0))
d = ImageDraw.Draw(img)


def sc(v):  # logical (1024-space) -> supersampled
    return int(v * S)


def pt(x, y):
    return (sc(x), sc(y))


def seg(p1, p2, width, color):
    d.line([pt(*p1), pt(*p2)], fill=color, width=int(width * S))


def cap(p, r, color):
    x, y = pt(*p)
    rr = r * S
    d.ellipse([x - rr, y - rr, x + rr, y + rr], fill=color)


def limb(p1, p2, width):
    seg(p1, p2, width, TEAL)
    cap(p1, width / 2, TEAL)
    cap(p2, width / 2, TEAL)


def joint(p, r=16):
    """Glowing joint node: soft halo rings + bright core."""
    x, y = pt(*p)
    for halo, alpha in ((2.6, 26), (1.9, 54), (1.35, 90)):
        rr = r * halo * S
        d.ellipse([x - rr, y - rr, x + rr, y + rr],
                  fill=GLOW + (alpha,))
    cap(p, r, NODE)


# --- background: navy rounded square ---
d.rounded_rectangle([sc(64), sc(64), sc(960), sc(960)],
                    radius=sc(176), fill=NAVY)

# --- skeleton puppet (all logical 1024-space) ---
head = (512, 296)
neck = (512, 408)
pelvis = (512, 596)

# head: big teal ellipse, slightly oblong
hx, hy = pt(*head)
hrx, hry = 92 * S, 98 * S
for halo, alpha in ((1.22, 36),):
    d.ellipse([hx - hrx * halo, hy - hry * halo,
               hx + hrx * halo, hy + hry * halo], fill=GLOW + (alpha,))
d.ellipse([hx - hrx, hy - hry, hx + hrx, hy + hry], fill=TEAL)
# face hint: two small navy eyes, flat style, no mouth
for ex in (-30, 30):
    exs = hx + ex * S
    rr = 9 * S
    ey = hy + 10 * S
    d.ellipse([exs - rr, ey - rr, exs + rr, ey + rr], fill=NAVY)

# spine
seg(neck, pelvis, 34, TEAL)
cap(neck, 17, TEAL)

# arms: right raised in a wave, left hanging loose
l_shoulder, l_elbow, l_hand = (448, 448), (392, 532), (424, 626)
r_shoulder, r_elbow, r_hand = (576, 448), (668, 380), (706, 268)
for a, b, c in ((l_shoulder, l_elbow, l_hand),
                (r_shoulder, r_elbow, r_hand)):
    limb(a, b, 30)
    limb(b, c, 30)
    cap(c, 22, TEAL)  # hand blob

# legs: left standing, right stepping out (lively pose)
l_hip, l_knee, l_ankle = (486, 604), (478, 730), (472, 858)
r_hip, r_knee, r_ankle = (538, 604), (606, 706), (664, 792)
for a, b, c in ((l_hip, l_knee, l_ankle), (r_hip, r_knee, r_ankle)):
    limb(a, b, 32)
    limb(b, c, 32)
    cap(c, 20, TEAL)  # foot blob
# feet point outward
seg(l_ankle, (432, 866), 24, TEAL)
cap((432, 866), 12, TEAL)
seg(r_ankle, (718, 800), 24, TEAL)
cap((718, 800), 12, TEAL)

# pelvis joint + all glowing joints
joint(pelvis, 20)
for p in (l_shoulder, r_shoulder, l_elbow, r_elbow,
          l_knee, r_knee, l_hip, r_hip):
    joint(p, 15)
for p in (l_hand, r_hand):
    joint(p, 13)

# --- downsample & save ---
out = img.resize((1024, 1024), Image.LANCZOS)
out.save("icon-1024.png")
out.save("icon.ico",
         sizes=[(16, 16), (24, 24), (32, 32), (48, 48),
                (64, 64), (128, 128), (256, 256)])
print("wrote icon-1024.png + icon.ico")