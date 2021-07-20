
# Running Smoke Tests for Forest's RPC API

## Prerequisites
In order to smoke test the RPC API endpoints, you need to meet 2 pre-requisites.
* A forest node should be running locally on port `1234`
* `FULLNODE_API_INFO` needs to be set as an environment variable. This can be done
   as a prefix to the actual command, set with a script that is sourced (linux),
   or set via the command line. If you don't know the multiaddr of the node, you can
   use `forest auth api-info` to get the key-value pair needed.

## Running the Tests
* Run `make smoke-test`

This will execute a blank request to all endpoints listed defined and check the HTTP
status code of the response. A good response is a 200. If a response is received, this
should be considered a good test, even if an error has occured. No parameters are passed
to the API endpoints. An `OK` will be displayed if a test passes.

## Adding Future Endpoints
Endpoints in the script `./scripts/smoke_test.sh` are stored in an array identified as `RPC_ENDPOINTS`.

Add the endpoint identifier minus the prefix `Forest` to the module that it belongs to (ie gas, net, state, etc)
or add a new section if a new API is added.

This should be checked during the review process if new API methods are added to keep this script and test up to date.
