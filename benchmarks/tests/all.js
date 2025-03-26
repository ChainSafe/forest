import http from "k6/http";

import { allMethods } from "../methods/index.js";
import { sendRpcRequest, assertSuccess } from "../utils/rpc.js";
import { regularBenchmarkParams } from "../utils/benchmark_params.js";

const url = __ENV.K6_TEST_URL || "http://localhost:2345/rpc/v1";

export let options = regularBenchmarkParams;

// the function that will be executed for each VU (virtual user)
export default function () {
  for (const method of allMethods) {
    const response = sendRpcRequest(url, method);
    assertSuccess(response);
  }
}
