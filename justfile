build:
    npm install
    just tailwind
    cargo build --release

tailwind:
    npx tailwindcss --input src/style.css --output dist/style.css

watch-rust:
    cargo watch -x run

watch-tailwind:
    cargo watch -w src/templates -s "just tailwind"

fmt:
    cargo fmt
    just fmt-templates

fmt-templates:
    djlint --reformat --profile jinja ./src/templates/

check-templates:
    djlint --lint --profile jinja ./src/templates/

docker-publish:
    ./docker-publish.sh

docker-build:
    ./docker-build.sh
