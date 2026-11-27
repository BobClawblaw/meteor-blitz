#!/usr/bin/env python3
"""Generate 1200x630 Open Graph / X social preview cards for review.html and index.html."""

from pathlib import Path
from PIL import Image, ImageDraw, ImageFilter, ImageFont

ROOT = Path(__file__).resolve().parent.parent
REVIEW = ROOT / "docs" / "review"
FONT_PATH = ROOT / "assets" / "fonts" / "Rajdhani-Bold.ttf"

W, H = 1200, 630


def fit_cover(img: Image.Image, tw: int, th: int, y_bias: float = 0.5) -> Image.Image:
    sw, sh = img.size
    scale = max(tw / sw, th / sh)
    nw, nh = round(sw * scale), round(sh * scale)
    resized = img.resize((nw, nh), Image.Resampling.LANCZOS)
    x0 = (nw - tw) // 2
    y0 = int(round((nh - th) * y_bias))
    return resized.crop((x0, y0, x0 + tw, y0 + th))


def add_bottom_scrim(base: Image.Image, start_y: int = 360, max_alpha: int = 235) -> Image.Image:
    overlay = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    px = overlay.load()
    span = H - start_y
    for y in range(start_y, H):
        t = (y - start_y) / span
        # Smoothstep curve for rich cinematic fade
        s = t * t * (3.0 - 2.0 * t)
        a = int(round(max_alpha * s))
        for x in range(W):
            px[x, y] = (6, 9, 18, a)
    return Image.alpha_composite(base, overlay)


def draw_pill(
    canvas: Image.Image,
    xy: tuple[int, int],
    text: str,
    font: ImageFont.FreeTypeFont,
    fill: tuple[int, int, int, int],
    border: tuple[int, int, int, int],
    text_color: tuple[int, int, int, int],
    align_right: bool = False,
) -> None:
    overlay = Image.new("RGBA", canvas.size, (0, 0, 0, 0))
    draw = ImageDraw.Draw(overlay)
    bbox = draw.textbbox((0, 0), text, font=font)
    tw, th = bbox[2] - bbox[0], bbox[3] - bbox[1]
    pad_x, pad_y = 18, 9
    pw, ph = tw + pad_x * 2, th + pad_y * 2
    x = xy[0] - pw if align_right else xy[0]
    y = xy[1]
    draw.rounded_rectangle([x, y, x + pw, y + ph], radius=ph // 2, fill=fill, outline=border, width=2)
    draw.text((x + pad_x - bbox[0], y + pad_y - bbox[1]), text, font=font, fill=text_color)
    canvas.alpha_composite(overlay)


def make_review_card() -> Path:
    # Crop out the top HUD strip and bottom achievement toast so the card showcases
    # pure lava/ice gameplay and leaves the top pills & bottom typography spotless.
    b_raw = Image.open(REVIEW / "before_world2.webp").convert("RGBA")
    a_raw = Image.open(REVIEW / "after_world2.webp").convert("RGBA")
    crop_box = (0, 90, b_raw.width, b_raw.height - 205)
    before = fit_cover(b_raw.crop(crop_box), W, H, y_bias=0.5)
    after = fit_cover(a_raw.crop(crop_box), W, H, y_bias=0.5)

    split_x = W // 2
    canvas = after.copy()
    canvas.paste(before.crop((0, 0, split_x, H)), (0, 0))

    # Subtle cyan glow + crisp divider line fading smoothly into the bottom scrim
    glow = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    gdraw = ImageDraw.Draw(glow)
    gdraw.line([(split_x, 0), (split_x, 420)], fill=(56, 189, 248, 210), width=10)
    glow = glow.filter(ImageFilter.GaussianBlur(radius=8))
    canvas.alpha_composite(glow)

    div = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    ddraw = ImageDraw.Draw(div)
    ddraw.line([(split_x, 0), (split_x, H)], fill=(255, 255, 255, 235), width=3)
    canvas.alpha_composite(div)

    # Bottom cinematic gradient scrim (fades divider smoothly into dark header strip)
    canvas = add_bottom_scrim(canvas, start_y=325, max_alpha=250)

    # Circular slider handle above the text strip
    handle = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    hdraw = ImageDraw.Draw(handle)
    cy = 215
    r = 26
    hdraw.ellipse([split_x - r - 6, cy - r - 6, split_x + r + 6, cy + r + 6], fill=(56, 189, 248, 85))
    hdraw.ellipse([split_x - r, cy - r, split_x + r, cy + r], fill=(7, 11, 22, 240), outline=(255, 255, 255, 255), width=3)
    hdraw.polygon([(split_x - 12, cy), (split_x - 4, cy - 7), (split_x - 4, cy + 7)], fill=(255, 255, 255, 255))
    hdraw.polygon([(split_x + 12, cy), (split_x + 4, cy - 7), (split_x + 4, cy + 7)], fill=(255, 255, 255, 255))
    canvas.alpha_composite(handle)

    f_pill = ImageFont.truetype(str(FONT_PATH), 23)
    f_eyebrow = ImageFont.truetype(str(FONT_PATH), 22)
    f_title = ImageFont.truetype(str(FONT_PATH), 58)
    f_sub = ImageFont.truetype(str(FONT_PATH), 25)

    draw_pill(
        canvas,
        (28, 24),
        "BEFORE  ·  v1.1.3 (xBR)",
        f_pill,
        fill=(127, 29, 29, 195),
        border=(248, 113, 113, 220),
        text_color=(254, 226, 226, 255),
    )
    draw_pill(
        canvas,
        (W - 28, 24),
        "AFTER  ·  v1.2 (Real-ESRGAN + PBR)",
        f_pill,
        fill=(20, 83, 45, 195),
        border=(74, 222, 128, 220),
        text_color=(220, 252, 231, 255),
        align_right=True,
    )

    # Bottom typography
    txt = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    tdraw = ImageDraw.Draw(txt)
    tdraw.text((44, 438), "GRAPHICS ARCHITECTURE POST-MORTEM", font=f_eyebrow, fill=(94, 234, 212, 255))
    tdraw.text((42, 466), "METEOR BLITZ — 8× GRAPHICS OVERHAUL", font=f_title, fill=(255, 255, 255, 255))
    tdraw.text(
        (44, 544),
        "Real-ESRGAN 8× Art  ·  Per-Pixel PBR Lighting  ·  5-Mip ACES Bloom  ·  GPU Gas Physics",
        font=f_sub,
        fill=(186, 204, 228, 255),
    )
    # Subtle accent bar at bottom edge
    tdraw.rectangle([0, H - 5, W, H], fill=(56, 189, 248, 255))
    canvas.alpha_composite(txt)

    out = REVIEW / "og-review.jpg"
    canvas.convert("RGB").save(out, "JPEG", quality=92, optimize=True, progressive=True)
    return out


def make_game_card() -> Path:
    bg = fit_cover(Image.open(REVIEW / "showcase_combat.webp").convert("RGBA"), W, H, y_bias=0.45)
    canvas = add_bottom_scrim(bg, start_y=350, max_alpha=240)

    f_eyebrow = ImageFont.truetype(str(FONT_PATH), 22)
    f_title = ImageFont.truetype(str(FONT_PATH), 64)
    f_sub = ImageFont.truetype(str(FONT_PATH), 26)

    txt = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    tdraw = ImageDraw.Draw(txt)
    tdraw.text((44, 434), "PLAY FREE IN BROWSER  ·  4K MACOS & WINDOWS", font=f_eyebrow, fill=(94, 234, 212, 255))
    tdraw.text((42, 462), "METEOR BLITZ", font=f_title, fill=(255, 255, 255, 255))
    tdraw.text(
        (44, 544),
        "Modern Rust Port of the 2009 Arcade Twin-Stick Shooter  ·  60–240 Hz  ·  Controller Support",
        font=f_sub,
        fill=(186, 204, 228, 255),
    )
    tdraw.rectangle([0, H - 5, W, H], fill=(56, 189, 248, 255))
    canvas.alpha_composite(txt)

    out = REVIEW / "og-game.jpg"
    canvas.convert("RGB").save(out, "JPEG", quality=92, optimize=True, progressive=True)
    return out


if __name__ == "__main__":
    p1 = make_review_card()
    p2 = make_game_card()
    print(f"Wrote {p1} ({p1.stat().st_size:,} bytes)")
    print(f"Wrote {p2} ({p2.stat().st_size:,} bytes)")
