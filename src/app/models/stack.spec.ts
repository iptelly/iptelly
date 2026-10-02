import { Node } from "./node";
import { NodeType } from "./nodeType";
import { Stack } from "./stack";

describe("Stack", () => {
  const category = new Node(1, "Sports", NodeType.Category);
  const series = new Node(2, "Some Show", NodeType.Series);
  const season = new Node(3, "Season 1", NodeType.Season);
  let stack: Stack;

  beforeEach(() => {
    stack = new Stack();
  });

  it("starts empty", () => {
    expect(stack.hasNodes()).toBe(false);
    expect(stack.get()).toBeUndefined();
  });

  it("throws when popping an empty stack", () => {
    expect(() => stack.pop()).toThrowError("Stack is empty");
  });

  it("returns the most recently added node from get without removing it", () => {
    stack.add(category);
    stack.add(series);
    expect(stack.get()).toBe(series);
    expect(stack.get()).toBe(series);
    expect(stack.hasNodes()).toBe(true);
  });

  it("pops nodes in last-in, first-out order", () => {
    stack.add(category);
    stack.add(series);
    stack.add(season);
    expect(stack.pop()).toBe(season);
    expect(stack.pop()).toBe(series);
    expect(stack.get()).toBe(category);
    expect(stack.pop()).toBe(category);
    expect(stack.hasNodes()).toBe(false);
  });

  it("clear empties the stack and returns the first node added", () => {
    stack.add(category);
    stack.add(series);
    expect(stack.clear()).toBe(category);
    expect(stack.hasNodes()).toBe(false);
    expect(stack.get()).toBeUndefined();
  });

  it("clear on an empty stack returns undefined", () => {
    expect(stack.clear()).toBeUndefined();
  });
});
