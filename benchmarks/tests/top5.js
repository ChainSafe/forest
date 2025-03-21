import { check } from "k6";
import http from "k6/http";

import { top5Methods } from "../methods/index.js";
import { assertSuccess, sendRpcRequest } from "../utils/rpc.js";
import { regularBenchmarkParams } from "../utils/benchmark_params.js";

const url = __ENV.K6_TEST_URL || "http://localhost:2345/rpc/v1";

export let options = regularBenchmarkParams;

// the function that will be executed for each VU (virtual user)
export default function () {
  for (const method of top5Methods) {
    const response = sendRpcRequest(url, method);
    assertSuccess(response);
  }
}
