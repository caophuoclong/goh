# Local Linux regression environment; AT-SPI stays inside the container.
FROM rust:1.99-bookworm
RUN apt-get update -qq && apt-get install -y --no-install-recommends \
    libxkbcommon-dev libxkbcommon-x11-0 libwayland-dev libgl1-mesa-dev libegl1-mesa-dev \
    xvfb xauth openbox xdotool x11-utils dbus-x11 python3-gi gir1.2-vte-2.91 at-spi2-core \
    sway xwayland zsh \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /work
ENV LIBGL_ALWAYS_SOFTWARE=1 NO_AT_BRIDGE=0 CARGO_TARGET_DIR=/work/target/linux
