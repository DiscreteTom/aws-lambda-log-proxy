export const handler = async (event) => {
  process.stdout.write(`${JSON.stringify(event)}\n`);
  process.stdout.write(`hello\n`);
  process.stderr.write("some err\n");

  return {
    statusCode: 200,
    body: "hello",
  };
};
