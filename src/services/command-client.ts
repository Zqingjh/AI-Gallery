export type CommandArguments = Record<string, unknown>;

export interface CommandClient {
  invoke<TResult>(
    command: string,
    arguments_?: CommandArguments,
  ): Promise<TResult>;
}

export type CommandInvoker = <TResult>(
  command: string,
  arguments_?: CommandArguments,
) => Promise<TResult>;
