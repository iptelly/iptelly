import { MediaType } from "./mediaType";
import { fromMediaType, NodeType } from "./nodeType";

describe("fromMediaType", () => {
  it("maps browsable media types to node types", () => {
    expect(fromMediaType(MediaType.group)).toBe(NodeType.Category);
    expect(fromMediaType(MediaType.serie)).toBe(NodeType.Series);
    expect(fromMediaType(MediaType.season)).toBe(NodeType.Season);
  });

  it("throws for media types that cannot be browsed into", () => {
    expect(() => fromMediaType(MediaType.livestream)).toThrowError("Invalid type: livestream");
    expect(() => fromMediaType(MediaType.movie)).toThrowError("Invalid type: movie");
  });
});
