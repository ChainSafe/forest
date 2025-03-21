import http from "k6/http";
import { check } from "k6";

export function sendRpcRequest(url, method) {
  const payload = JSON.stringify({
    jsonrpc: "2.0",
    method: method.name,
    params: method.params,
    id: 2,
  });
  const headers = {
    "Content-Type": "application/json",
  };

  // Note: the tag is used to identify the request in the results, given all requests are hitting the same endpoint.
  const response = http.post(url, payload, {
    headers,
    tags: { name: method.name },
  });

  if (__ENV.K6_TEST_DEBUG === "true" || __ENV.K6_TEST_DEBUG === "1") {
    console.log(`Response for ${method.name}: ${response.body}`);
  }

  return response;
}

export function assertSuccess(response) {
  check(response, {
    "is status 200": (r) => r.status === 200,
    "is JSON-RPC without error": (r) => {
      const res = JSON.parse(r.body);
      return !("error" in res);
    },
  });
}
