#!/usr/bin/env python3
import re
import PIL
from PIL import Image, ImageFont, ImageOps, ImageDraw


class AnsiEscapeToken:
    Colors = [
        "#111111",
        "#FF4136",
        "#2ECC40",
        "#FFDC00",
        "#0074D9",
        "#F012BE",
        "#39CCCC",
        "#DDDDDD",
    ]

    def __init__(self, string):
        self.string = string
        self.seq = [int(s) for s in re.findall(r"\d+", self.string)]

    def __repr__(self):
        return repr(self.string)

    def __str__(self):
        return ""

    def __len__(self):
        return 0

    def fill(self):
        for code in self.seq:
            if 30 <= code < 38:
                hexstr = self.Colors[code - 30].lstrip("#")
                return tuple(int(hexstr[i : i + 2], 16) for i in (0, 2, 4))
        return (255, 255, 255)


def pt2px(pt):
    return int(round(pt * 96.0 / 72))


def main():
    with open("tt.txt") as file:
        lines = [l.rstrip() for l in file.readlines()]

    escape = re.compile(r"\x1b\[\d+(?:;\d+)*m")

    rows = []
    for line in lines:
        row = []
        start = pos = 0
        while token := escape.search(line, pos):
            end, pos = token.span()
            row.append(line[start:end])
            row.append(AnsiEscapeToken(token[0]))
            start = pos
        if start < len(line):
            row.append(line[start:])
        rows.append(row)

    font_path = "CascadiaMonoNF-SemiLight.otf"
    try:
        font = ImageFont.truetype(font_path, size=20)
    except IOError:
        font = PIL.ImageFont.load_default()
        print("Could not use chosen font. Using default.")

    width = pt2px(max(font.getsize("".join(str(x) for x in row))[0] for row in rows))
    line_height = int(round(pt2px(font.getsize("hyrious")[1]) * 0.8))

    image = Image.new("RGB", (width, line_height * (1 + len(rows))), color=0x121212)
    draw = ImageDraw.Draw(image)

    y = 5
    fill = (255, 255, 255)
    for row in rows:
        x = 5
        for token in row:
            if isinstance(token, AnsiEscapeToken):
                fill = token.fill()
            else:
                draw.text((x, y), token, font=font, fill=fill)
                x += pt2px(font.getsize(token)[0])
        y += line_height

    box = ImageOps.invert(image).getbbox()
    image.crop(box).save("tt.png")


if __name__ == "__main__":
    main()
