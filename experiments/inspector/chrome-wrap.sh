#!/bin/bash
exec /usr/bin/google-chrome --headless=new --no-sandbox --window-size=390,844 --force-device-scale-factor=1 "$@"
