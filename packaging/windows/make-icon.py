"""Draw the app icon: a white invoice sheet on a dark rounded tile.

Writes assets/icon.png (window icon) and assets/icon.ico (exe + installer).
Run from the repo root: python packaging/windows/make-icon.py  (needs Pillow)
"""

from PIL import Image, ImageDraw

S = 1024  # draw large, then downscale for smooth edges


def draw() -> Image.Image:
    img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    d.rounded_rectangle([40, 40, S - 40, S - 40], radius=200, fill=(26, 26, 26, 255))

    # Sheet with a folded top-right corner.
    left, top, right, bottom, fold = 270, 190, 754, 834, 140
    d.polygon(
        [(left, top), (right - fold, top), (right, top + fold), (right, bottom), (left, bottom)],
        fill=(255, 255, 255, 255),
    )
    d.polygon([(right - fold, top), (right - fold, top + fold), (right, top + fold)], fill=(200, 200, 200, 255))

    # Text lines, then a bold total.
    ink = (150, 150, 150, 255)
    for i, width in enumerate([300, 220, 300, 260]):
        y = 360 + i * 80
        d.rounded_rectangle([left + 70, y, left + 70 + width, y + 28], radius=14, fill=ink)
    d.rounded_rectangle([left + 214, 700, right - 70, 748], radius=18, fill=(26, 26, 26, 255))
    return img


def main() -> None:
    img = draw()
    img.resize((256, 256), Image.LANCZOS).save("assets/icon.png")
    img.save("assets/icon.ico", sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])


if __name__ == "__main__":
    main()
