using System.Text.Json;

namespace RedRepublic.Sim.Tests;

/// <summary>
/// The two files the suite checks against, found relative to the repository
/// rather than copied beside the binary.
/// </summary>
/// <remarks>
/// <para>
/// <c>game/data/manifest.json</c> is the file the shipped game loads, and
/// <c>Data/vectors.json</c> is what the Rust reference produced. Reading the
/// real ones means the suite cannot pass against a stale copy that a build step
/// forgot to refresh — which is the failure a "copy to output directory" would
/// introduce and nothing would report.
/// </para>
/// <para>
/// Loading is done once per test run and shared, because parsing the manifest
/// and generating maps is the expensive part and xUnit builds a fresh test class
/// instance per test method.
/// </para>
/// </remarks>
public static class Fixtures
{
    private static readonly Lazy<string> RepoRootLazy = new(FindRepoRoot);
    private static readonly Lazy<Tables> TablesLazy = new(() =>
        Tables.Load(File.ReadAllText(Path.Combine(RepoRoot, "game", "data", "manifest.json"))));

    private static readonly Lazy<JsonDocument> VectorsLazy = new(() =>
        JsonDocument.Parse(File.ReadAllText(
            Path.Combine(RepoRoot, "src", "RedRepublic.Sim.Tests", "Data", "vectors.json"))));

    public static string RepoRoot => RepoRootLazy.Value;

    public static Tables Tables => TablesLazy.Value;

    public static JsonElement Vectors => VectorsLazy.Value.RootElement;

    /// <summary>Sixteen hex digits to a <see cref="ulong"/>.</summary>
    /// <remarks>
    /// The vectors carry every 64-bit value and every double as a bit pattern in
    /// hex, not as a JSON number. Two reasons, both measured rather than
    /// theoretical: this repository found that a JSON parser is not necessarily
    /// correctly rounded — serde_json returned a different value for 91,767 of
    /// 200,000 sampled doubles — and a JSON number is a double on arrival, so
    /// the integer 9007199254740993 comes back as ...992 and silently pins a
    /// different case than the one chosen.
    /// </remarks>
    public static ulong Hex(string s) => Convert.ToUInt64(s, 16);

    public static double BitsToDouble(string hex) => BitConverter.UInt64BitsToDouble(Hex(hex));

    /// <summary>
    /// Walks up from the test binary until it finds the repository. Not a
    /// hard-coded path and not an environment variable: either would make the
    /// suite pass or fail depending on where it was run from.
    /// </summary>
    private static string FindRepoRoot()
    {
        var dir = new DirectoryInfo(AppContext.BaseDirectory);
        while (dir is not null)
        {
            if (Directory.Exists(Path.Combine(dir.FullName, ".git")))
            {
                return dir.FullName;
            }

            dir = dir.Parent;
        }

        throw new DirectoryNotFoundException(
            $"no repository root above {AppContext.BaseDirectory}");
    }
}
