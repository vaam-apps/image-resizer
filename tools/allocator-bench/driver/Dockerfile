# Load driver / verification client for #144's allocator measurement.
FROM python:3.12-slim

RUN pip install --no-cache-dir aiohttp==3.10.11

WORKDIR /driver
COPY --chown=65532:65532 load_driver.py fetch_one.py ./

# Trivy DS-0002. Generating HTTP load needs no privilege.
RUN groupadd -g 65532 bench && useradd -u 65532 -g bench -M -s /usr/sbin/nologin bench
USER 65532:65532

# Trivy DS-0026 is deliberately unsatisfied: this is a one-shot client that
# runs a fixed workload and exits, not a service. A HEALTHCHECK on a container
# whose correct behaviour is to terminate would report unhealthy exactly when
# it has succeeded.

ENTRYPOINT ["python3"]
