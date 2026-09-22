# Load driver / verification client for #144's allocator measurement.
FROM python:3.12-slim

RUN pip install --no-cache-dir aiohttp==3.10.11

WORKDIR /driver
COPY load_driver.py fetch_one.py ./

ENTRYPOINT ["python3"]
