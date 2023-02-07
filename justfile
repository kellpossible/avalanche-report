tailwind:
    npx tailwindcss --input src/style.css --output dist/style.css

watch-rust:
    cargo watch -x run

watch-tailwind:
    cargo watch -w src/templates -s "just tailwind"

fmt-templates:
    djlint --reformat --profile jinja ./src/templates/

check-templates:
    djlint --lint --profile jinja ./src/templates/
