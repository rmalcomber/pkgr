import { Command } from "@cliffy/command";
import { task_select } from "./commands/task_select.ts";

await new Command()
  // Main command.
  .name("pkgr")
  .version("0.1.0")
  .arguments("<value:string>")
  .action((options, ...args) => {
    console.log("Main command called.", args);
  })
  // Child command 1.
  .command("foo", task_select)
  // Child command 2.
  .command("bar", "Bar sub-command.")
  .option("-b, --bar", "Bar option.")
  .arguments("<input:string> [output:string]")
  .action((options, ...args) => console.log("Bar command called."))
  .parse();
