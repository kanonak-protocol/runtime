#!/usr/bin/env bash
# Puts Maven 3.9.16 on the job's PATH, verified by its Apache-published sha512. publish-java builds the Central bundle
# with it, and test-java resolves the release profile's plugins with it, so both see the same plugins.
set -euo pipefail
v=3.9.16
tgz="$RUNNER_TEMP/apache-maven-$v-bin.tar.gz"
curl -sfL -o "$tgz" "https://archive.apache.org/dist/maven/maven-3/$v/binaries/apache-maven-$v-bin.tar.gz"
echo "831a8591fe20c8243b1dbe7d71e3244f31d1665b0804b2e825e38cbbe5ce0cafb8338851f90780735568773e0a6cd07bbec107cda0b896b008b861075358b6f6  $tgz" | sha512sum -c --quiet -
tar -xzf "$tgz" -C "$RUNNER_TEMP"
echo "$RUNNER_TEMP/apache-maven-$v/bin" >> "$GITHUB_PATH"
