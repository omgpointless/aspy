#!/bin/bash
# Local Jekyll development server
# Usage: ./serve.sh [--future]

cd "$(dirname "$0")"

docker run --rm \
  -v "$(pwd):/srv/jekyll" \
  -p 4000:4000 \
  jekyll/jekyll \
  sh -c "bundle install && bundle exec jekyll serve --host 0.0.0.0 $*"