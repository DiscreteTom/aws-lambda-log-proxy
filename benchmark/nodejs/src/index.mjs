import { faker } from "@faker-js/faker";

const LOG_COUNT = Number(process.env.LOG_COUNT);

export const handler = async () => {
  for (let i = 0; i < LOG_COUNT; ++i) {
    // use process.stdout to avoid prepending other info
    process.stdout.write(faker.git.commitEntry() + "\n");
  }

  process.stdout.write(`done\n`);

  return {
    statusCode: 200,
    body: "hello",
  };
};
