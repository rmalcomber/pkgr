import { Command } from "@cliffy/command";
import { assert } from "@std/assert";
import { isAbsolute, join, parse } from "@std/path";
import { exists } from "@std/fs";
import { Select } from "@cliffy/prompt/select";

export const task_select = new Command()
  .arguments("<input:string>")
  .action(action);

export async function action(_options: unknown, input: string) {
  assert(input, "Missing input");

  let fullPath = isAbsolute(input) ? input : join(Deno.cwd(), input);

  const endWithJson = fullPath.endsWith(".json");

  if (!endWithJson) {
    const packageJson = join(fullPath, "package.json");
    if (await exists(packageJson)) {
      fullPath = packageJson;
    } else {
      // Try Deno
      const packageDeno = join(fullPath, "deno.json");
      if (await exists(packageDeno)) {
        fullPath = packageDeno;
      } else {
        throw new Error("Can't find package.json or deno.json");
      }
    }
  }

  const pathDetails = parse(fullPath);
  const isPackageJson = pathDetails.name === "package";

  const fileContents = await Deno.readTextFile(fullPath);

  const json = JSON.parse(fileContents);

  const objectTasks = isPackageJson ? json["scripts"] : json["tasks"];

  const tasks: { name: string; command: string }[] = [];

  for (const t of Object.keys(objectTasks)) {
    tasks.push({ name: t, command: objectTasks[t] });
  }

  const selectedTask = await Select.prompt({
    message: "Select a task/script",
    options: tasks.map((t) => ({
      name: `${t.name}: ${t.command}`,
      value: t.name,
    })),
  });

  const command = isPackageJson
    ? `npm run "${selectedTask}"`
    : `deno task "${selectedTask}"`;

  new Deno.Command("cmd.exe", {
    args: [
      "/d",
      "/s",
      "/c",
      `start "" /b ${command}`,
    ],
    cwd: pathDetails.dir,
    stdin: "null",
    stdout: "null",
    stderr: "null",
  }).spawn();

  Deno.exit(0);
}
