import time
import random
from pip._vendor import requests
import string

def generate_random_value():
    """
    Generate a random value that can be:
    1. A random text string (alphanumeric).
    2. A random number.
    3. A formula in the format `=A1000`.
    """
    choice = random.choice(["text", "number", "formula"])

    if choice == "text":
        # Generate a random alphanumeric text string
        length = random.randint(5, 15)  # Random length between 5 and 15
        return ''.join(random.choices(string.ascii_letters + string.digits, k=length))

    elif choice == "number":
        # Generate a random integer
        return str(random.randint(1, 10000))

    elif choice == "formula":
        # Generate a formula in the format =A1000
        letter = random.choice(string.ascii_uppercase)  # Random uppercase letter A-Z
        number = random.randint(1, 10000)  # Random number
        return f"={letter}{number}"

def make_cell(ide: int, raw_value: str, background: int):
    return {
        "id": ide,
        "raw_value": raw_value,
        "background": background,
    }

def lambda_handler(event, context):
    """
    Lambda function handler that sends POST requests to a given URL
    for a limited duration.

    Event structure:
    {
        "url": "http://localhost:3000/api/spreadsheet",
        "duration": 10, # Duration of load-test in seconds
        "interval": 0.1 # Time between requests in seconds
        cell_start: 0, # Start cell id for range
        cell_end: 100 # End cell id for range
    }
    """
    # Read parameters from the event
    url = event["url"]
    duration = event.get("duration", 10)
    interval = event.get("interval", 0.1)
    cell_start = event.get("cell_start", 0)
    cell_end = event.get("cell_end", 10000)

    headers = {"Content-Type": "application/json"}

    # Validate URL
    if not url:
        return {"status": "error", "message": "URL is required"}

    start_time = time.time()
    responses = []

    with requests.Session() as session:
        # Perform POST requests in a loop for the given duration
        while time.time() - start_time < duration:
            try:
                data = generate_random_value()
                cell = make_cell(random.randint(cell_start, cell_end), data, random.randint(0, 16777215))

                response = session.post(url, json=cell, headers=headers)
                responses.append({
                    "status_code": response.status_code,
                    "body": response.text,
                })
            except requests.RequestException as e:
                responses.append({"error": str(e)})

            if interval > 0:
                time.sleep(interval)

    return {
        "requests_made": len(responses),
        "responses": responses,
    }

if __name__ == '__main__':
    total = 0
    resp = lambda_handler({
        "url": "http://localhost:3000/api/spreadsheet",
        "duration": 10,
        "cell_start": 0,
        "cell_end": 1000,
        "interval": 0}, None)

    failed = len(list(filter(lambda x: x["status_code"] != 200 or 'error' in x, resp["responses"])))
    total += resp["requests_made"]
    if failed > 0:
        print(resp["responses"])
    print("Total {} req completed ({} failures)".format(total, failed))
