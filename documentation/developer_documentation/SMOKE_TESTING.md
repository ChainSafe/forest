
# Running Smoke Tests for Forest's RPC API

## Prerequisites
The only requirement for running these smoke tests is that Forest is installed and on your system PATH.

## Running the Tests
* Use `make install` to create a binary on your path
* Run `make smoke-test`

This will execute a blank request to all endpoints listed defined and check the HTTP
status code of the response. If a response is received, this should be considered a good
test, even if an error has occurred. No parameters are passed to the API endpoints.
An `OK` will be displayed if a test passes, and a `FAIL` will be displayed with an HTTP/curl code
if a test fails.

## Adding Future Endpoints
Endpoints in the script `./scripts/smoke_test.sh` are stored in an array identified as `RPC_ENDPOINTS`.

Add the endpoint identifier minus the prefix `Forest` to the module that it belongs to (ie gas, net, state, etc)
or add a new section if a new API is added.

This should be checked during the review process if new API methods are added to keep this script and test suite up to date.
