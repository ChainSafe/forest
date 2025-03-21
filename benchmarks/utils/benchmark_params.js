// Default benchmark parameters. These can be overridden via CLI or environment variables.
export let regularBenchmarkParams = {
  stages: [
    { duration: "10s", target: 100 }, // simulate ramp-up of users over 10 seconds. This might allow for caches to warm up.
    { duration: "1m", target: 100 }, // keep 100 users for 1 minute. This is an arbitrary peak value. Note: it should be adjusted to the computational power of the machine running the test.
    { duration: "10s", target: 0 }, // ramp down to 0 users
  ],
};
