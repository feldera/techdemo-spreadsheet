from PIL import Image
import requests
import struct
import sys

def make_cell(ide: int, raw_value: str, background: int):
    return {
        "id": ide,
        "raw_value": raw_value,
        "background": background,
    }

def image_to_excel(
        input_image_path: str,
        max_width: int = 26,
        row_start: int = 0
):
    # 1. Load the image
    img = Image.open(input_image_path).convert("RGBA")

    # 2. Calculate the new height keeping aspect ratio
    width_percent = max_width / float(img.width)
    print(width_percent)
    new_height = int(float(img.height*2) * width_percent)

    # 3. Resize the image
    img = img.resize((max_width, new_height), Image.Resampling.LANCZOS)


    with requests.Session() as session:
        try:
            headers = {"Content-Type": "application/json"}
            # 5. For each pixel, create a cell with the corresponding fill color
            error_count = 0
            for row in range(new_height):
                for col in range(max_width):
                    r, g, b, a = img.getpixel((col, row))
                    rgba_32 = struct.unpack('i', struct.pack('>4B', r, g, b, a))[0]
                    idx = row_start*26+row*26+col
                    cell = make_cell(idx, "", rgba_32)
                    response = session.post("https://xls.fly.dev/api/spreadsheet", json=cell, headers=headers)
                    if response.status_code != 200:
                        error_count += 1
        except requests.RequestException as e:
            raise e
        if error_count > 0:
            print(f"Failed to send {error_count} cells")




if __name__ == "__main__":
    img = sys.argv[1]
    max_width = int(sys.argv[2])
    row_start = int(sys.argv[3])

    # Example usage:
    image_to_excel(
        input_image_path=img,
        max_width=max_width,
        row_start=row_start
    )
