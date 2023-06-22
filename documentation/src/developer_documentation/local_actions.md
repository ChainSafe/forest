There are often cases where the jobs fail at the CI and not locally, which tends
to be cumbersome to debug. Also, when developing an integration test, it is
useful to get immediate feedback instead of relying on Github Actions (which, on
a side note, are sometimes down).

There is a [tool called Act](https://github.com/nektos/act) that allows you to
run Github Actions locally. Given the complexity of Forest's CI, it is difficult
to run the whole CI locally, but it is feasible to run a single or set of jobs.
This is useful to debug a failing job, or to run an integration test locally.
Note that while Github Actions are run in fully virtualized environments, Act
runs them in Docker containers. This means that the environment is not exactly
the same, but it is close enough to be useful. In practice, we need some
_tricks_ to make it work.

# Installation

To install Act, follow the instructions specific to your OS in the
[Act repository](https://github.com/nektos/act#installation-through-package-managers).

On the first run, `act` will ask you to pick the image size. Either choose the
biggest one (it's ~60GiB unzipped) or the medium one. The large one should have
fewer issues with the missing software, but it will take longer to download and
is based on an older Ubuntu base. You can always edit this later in
`$HOME/.actrc`.

# Challenges

Let's consider running an integration test. At the time of writing, the usual
workflow looks like this:

1. Build Forest daemon and CLI in one job.
2. Upload the artifacts to GH.
3. In another job, download the artifacts to the test runner.
4. Run the integration test.

There are some hurdles to overcome.

## sccache

### Disabling sccache

We have it everywhere where compilation is involved. It is not installed in the
Act container, so we need to comment all such jobs out. It's not really
mandatory if the `continue-on-error` is set to `true` but it does unclutter the
logs.

```yaml
- name: Setup sccache
  uses: mozilla-actions/sccache-action@v0.0.3
  timeout-minutes: ${{ fromJSON(env.CACHE_TIMEOUT_MINUTES) }}
  continue-on-error: true
```

A mandatory step is to disable the `sccache` in the compiler variables. Comments
those overrides out.

```yaml
RUSTC_WRAPPER: "sccache"
CC: "sccache clang"
CXX: "sccache clang++"
```

### Installing sccache

Alternatively, if debugging `sccache` itself you can set it up yourself. Create
your own Space in Digital Ocean. Create a `.env` file and add the following
variables there:

```
SCCACHE_BUCKET=<my-personal-bucket> SCCACHE_REGION=auto
SCCACHE_ENDPOINT=<my-personal-endpoint>
```

Grab your Digital Ocean access token and add it to a secrets file. Make sure you
don't commit it to the project!

```
AWS_ACCESS_KEY_ID=<my-personal-access-key-id>
AWS_SECRET_ACCESS_KEY=<my-personal-secret-access-key>
```

You will be able to use those files with the `--env-file` and `--secret-file`
flags.

On top of that, you will need to manually install `sccache` in the container.
Grab the URL of the latest release from the
[sccache repository](https://github.com/mozilla/sccache/releases) and put it as
a step in the job that needs it.

```shell
wget https://github.com/mozilla/sccache/releases/download/v0.5.3/sccache-v0.5.3-x86_64-unknown-linux-musl.tar.gz
tar -zxf sccache-v0.5.3-x86_64-unknown-linux-musl.tar.gz
sudo mv sccache-v0.5.3-x86_64-unknown-linux-musl/sccache /usr/bin/ && sudo chmod +x /usr/bin/sccache
```

## Uploading/downloading artifacts

If your job uses one of those actions, you can support it with the
`--artifact-server-path <temporary-path`. Make sure the directory is created
before running the job.

## Missing commands

Some commands are not available in the Act container. You can either install
them manually or disable such steps. For example, the `lscpu` command is not
available.

## Missing certificates

It may happen for some downloads. You can disable the step or install the
certificates manually. For example, the `rustup` command fails because of that.
You can install the certificates with the following command:

```shell
apt-get install -y ca-certificates
```

If this does not work, you can try to install the certificates manually, for
example, if there are issues with
[LetsEncrypt](https://letsencrypt.org/certificates/), you try downloading a new
root certificate.

```shell
wget https://letsencrypt.org/certs/isrgrootx1.pem
mv isrgrootx1.pem /usr/local/share/ca-certificates/isrgrootx1.crt update-ca-certificates --fresh
```

## `cargo` not in PATH

Add it to the PATH manually before running the command that requires it:

```yaml
run: |
  export PATH="${HOME}/.cargo/bin:${PATH}"
  make install
```

## Rebuilding Forest from scratch

You can avoid re-building the entire project all the time either by re-using the
container with `--reuse` or by modifying the job to not depend on it and just
download the artifacts.

# Example run

After all the remarks above are addressed, you can run the job locally. For
example, to run the integration test for the CLI:

```shell
act --secret-file act-secrets.env --env-file act.env -W .github/workflows/forest.yml -j forest-cli-check --artifact-server-path /tmp/artifacts/  --reuse
```

Assuming you don't want to use `sccache` and have disabled it, you can run:

```shell
act -W .github/workflows/forest.yml -j forest-cli-check --artifact-server-path /tmp/artifacts/  --reuse
```

Shortened output:

```
❯ act --secret-file ../forest/act-secrets.env --env-file ../forest/act.env -W .github/workflows/forest.yml -j forest-cli-check --artifact-server-path /tmp/artifacts/  --reuse -q
INFO[0000] Start server on http://192.168.1.10:34567
[Integration tests/Build Ubuntu] 🚀  Start image=catthehacker/ubuntu:act-latest
[Integration tests/Build Ubuntu]   🐳  docker pull image=catthehacker/ubuntu:act-latest platform= username= forcePull=false
[Integration tests/Build Ubuntu]   🐳  docker create image=catthehacker/ubuntu:act-latest platform= entrypoint=["/usr/bin/tail" "-f" "/dev/null"] cmd=[]
[Integration tests/Build Ubuntu]   🐳  docker run image=catthehacker/ubuntu:act-latest platform= entrypoint=["/usr/bin/tail" "-f" "/dev/null"] cmd=[]
[Integration tests/Build Ubuntu]   ☁  git clone 'https://github.com/actions/upload-artifact' # ref=v3
[Integration tests/Build Ubuntu] ⭐ Run Main Show IP
[Integration tests/Build Ubuntu]   🐳  docker exec cmd=[bash --noprofile --norc -e -o pipefail /var/run/act/workflow/0] user= workdir=
[Integration tests/Build Ubuntu]   ✅  Success - Main Show IP
[Integration tests/Build Ubuntu] ⭐ Run Main Checkout Sources
[Integration tests/Build Ubuntu]   🐳  docker cp src=/home/rumcajs/prj/forest/. dst=/home/rumcajs/prj/forest
[Integration tests/Build Ubuntu]   ✅  Success - Main Checkout Sources
[Integration tests/Build Ubuntu] ⭐ Run Main Install Apt Dependencies
[Integration tests/Build Ubuntu]   🐳  docker exec cmd=[bash --noprofile --norc -e -o pipefail /var/run/act/workflow/2] user= workdir=
[Integration tests/Build Ubuntu]   ✅  Success - Main Install Apt Dependencies
[Integration tests/Build Ubuntu] ⭐ Run Main Cargo Install
[Integration tests/Build Ubuntu]   🐳  docker exec cmd=[bash --noprofile --norc -e -o pipefail /var/run/act/workflow/3] user= workdir=
[Integration tests/Build Ubuntu]   ✅  Success - Main Cargo Install
[Integration tests/Build Ubuntu] ⭐ Run Main actions/upload-artifact@v3
[Integration tests/Build Ubuntu]   🐳  docker cp src=/home/rumcajs/.cache/act/actions-upload-artifact@v3/ dst=/var/run/act/actions/actions-upload-artifact@v3/
[Integration tests/Build Ubuntu]   🐳  docker exec cmd=[node /var/run/act/actions/actions-upload-artifact@v3/dist/index.js] user= workdir=
[Integration tests/Build Ubuntu]   ✅  Success - Main actions/upload-artifact@v3
[Integration tests/Build Ubuntu] 🏁  Job succeeded
[Integration tests/Forest CLI checks] 🚀  Start image=catthehacker/ubuntu:act-latest
[Integration tests/Forest CLI checks]   🐳  docker pull image=catthehacker/ubuntu:act-latest platform= username= forcePull=false
[Integration tests/Forest CLI checks]   🐳  docker create image=catthehacker/ubuntu:act-latest platform= entrypoint=["/usr/bin/tail" "-f" "/dev/null"] cmd=[]
[Integration tests/Forest CLI checks]   🐳  docker run image=catthehacker/ubuntu:act-latest platform= entrypoint=["/usr/bin/tail" "-f" "/dev/null"] cmd=[]
[Integration tests/Forest CLI checks]   ☁  git clone 'https://github.com/actions/download-artifact' # ref=v3
[Integration tests/Forest CLI checks] ⭐ Run Main Checkout Sources
[Integration tests/Forest CLI checks]   🐳  docker cp src=/home/rumcajs/prj/forest/. dst=/home/rumcajs/prj/forest
[Integration tests/Forest CLI checks]   ✅  Success - Main Checkout Sources
[Integration tests/Forest CLI checks] ⭐ Run Main actions/download-artifact@v3
[Integration tests/Forest CLI checks]   🐳  docker cp src=/home/rumcajs/.cache/act/actions-download-artifact@v3/ dst=/var/run/act/actions/actions-download-artifact@v3/
[Integration tests/Forest CLI checks]   🐳  docker exec cmd=[node /var/run/act/actions/actions-download-artifact@v3/dist/index.js] user= workdir=
[Integration tests/Forest CLI checks]   ✅  Success - Main actions/download-artifact@v3
[Integration tests/Forest CLI checks]   ⚙  ::set-output:: download-path=/root/.cargo/bin
[Integration tests/Forest CLI checks] ⭐ Run Main Set permissions
[Integration tests/Forest CLI checks]   🐳  docker exec cmd=[bash --noprofile --norc -e -o pipefail /var/run/act/workflow/2] user= workdir=
[Integration tests/Forest CLI checks]   ✅  Success - Main Set permissions
[Integration tests/Forest CLI checks] ⭐ Run Main install CA certificates
[Integration tests/Forest CLI checks]   🐳  docker exec cmd=[bash --noprofile --norc -e -o pipefail /var/run/act/workflow/3] user= workdir=
[Integration tests/Forest CLI checks]   ✅  Success - Main install CA certificates
[Integration tests/Forest CLI checks] ⭐ Run Main Make sure everything is in PATH
[Integration tests/Forest CLI checks]   🐳  docker exec cmd=[bash --noprofile --norc -e -o pipefail /var/run/act/workflow/4] user= workdir=/root/.cargo/bin/
[Integration tests/Forest CLI checks]   ✅  Success - Main Make sure everything is in PATH
[Integration tests/Forest CLI checks] ⭐ Run Main forest-cli check
[Integration tests/Forest CLI checks]   🐳  docker exec cmd=[bash --noprofile --norc -e -o pipefail /var/run/act/workflow/5] user= workdir=
[Integration tests/Forest CLI checks]   ✅  Success - Main forest-cli check
[Integration tests/Forest CLI checks] 🏁  Job succeeded
```
