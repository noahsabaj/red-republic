using System.Reflection;

namespace RedRepublic.Sim.Tests;

/// <summary>
/// Nothing the simulation knows is invisible.
/// </summary>
/// <remarks>
/// <para>
/// <b>One guard now, where there used to be two.</b> The retired build marshalled
/// the simulation across an FFI boundary into a second language, so a fact could
/// reach the shell and stop there — and it did, twice, for five and six
/// milestones. There is no middle layer any more: the game project reads the
/// simulation directly, so "is this fact exposed" and "does a screen reach it"
/// are the same question and one scan answers it.
/// </para>
/// <para>
/// <b>It is a source scan, and that has a known blind spot.</b> A scan over text
/// finds the name it is looking for in a file that does not compile, and reports
/// the fact reached. What covers that is CI actually loading the game — the two
/// checks are not redundant and neither replaces the other.
/// </para>
/// <para>
/// <see cref="NotYetReached"/> is a <b>work list</b>. Wiring one of its entries
/// up fails this test until its line goes, which is the point: a list that can be
/// added to without friction is a list that only ever grows.
/// </para>
/// </remarks>
public sealed class ExposureTests
{
    /// <summary>
    /// Facts the simulation knows and no screen shows yet.
    /// </summary>
    /// <remarks>
    /// Every line here is a thing a player cannot see. It is not a list of
    /// exemptions — it is the outstanding half of condition one, and it is meant
    /// to reach nothing.
    /// </remarks>
    private static readonly string[] NotYetReached =
    [
        // The pollution and wear lattice, and the overlays that would draw it.
        "Lattice",

        // The geology under the map: what is down there, how deep, how much is
        // left. There is no survey overlay, so a player sites a mine blind.
        "Geology",

        // Visitors, and what draws them.
        "Tourism",

        // Where sites buy what the republic cannot make.
        "BuildPolicy",

        // What the republic was founded on. There is no founding screen, so the
        // seed and the size are not a player's to choose yet.
        "Spec",

        // The generator's stream position. Machinery rather than a fact, and it
        // is here rather than exempted because a save screen will show a
        // republic's identity and this is part of it.
        "Rng",

        // The record of every run opened, which a road overlay would draw.
        "Roadbook",

        // The five networks that are not roads. Nothing lays rail, tramway,
        // metro or air yet, and the waterways are sampled off the terrain and
        // drawn by nothing.
        "Rails",
        "Tramway",
        "Metro",
        "Waterways",
        "Airways",
    ];

    /// <summary>
    /// Every fact a republic has is read by the game, or is on the work list
    /// saying it is not.
    /// </summary>
    /// <remarks>
    /// <b>The facts are the public properties of <see cref="World"/></b> — what a
    /// republic <i>is</i>. Deliberately not every public type in the assembly:
    /// a mutation kind, a random generator and a run-length table are machinery
    /// the simulation needs to work, not things a player could look at, and a
    /// guard that demanded a screen for each would be one nobody could satisfy
    /// and everybody would exempt around.
    /// </remarks>
    [Fact]
    public void Every_fact_a_republic_has_is_reached_or_is_on_the_work_list()
    {
        var game = GameSource();
        var unreached = new List<string>();

        foreach (var fact in typeof(World).GetProperties(BindingFlags.Public | BindingFlags.Instance))
        {
            if (game.Contains($".{fact.Name}", StringComparison.Ordinal))
            {
                continue;
            }

            unreached.Add(fact.Name);
        }

        var missing = unreached.Except(NotYetReached).ToList();
        Assert.True(
            missing.Count == 0,
            "the simulation knows these and no screen shows them: "
            + string.Join(", ", missing)
            + " — either wire one up, or put it on NotYetReached with a line saying why");
    }

    /// <summary>
    /// The work list only shrinks.
    /// </summary>
    /// <remarks>
    /// A line that names something now reached is a line that would let the next
    /// omission hide behind it. Wiring one up therefore fails the build until the
    /// entry goes, which is the friction that makes the list a work list rather
    /// than a graveyard.
    /// </remarks>
    [Fact]
    public void The_work_list_names_nothing_that_is_already_reached()
    {
        var game = GameSource();
        var stale = NotYetReached
            .Where(name => game.Contains($".{name}", StringComparison.Ordinal))
            .ToList();

        Assert.True(
            stale.Count == 0,
            "these are on the work list and are reached after all: "
            + string.Join(", ", stale)
            + " — delete their lines");
    }

    /// <summary>
    /// The simulation exposes no engine type at its boundaries.
    /// </summary>
    /// <remarks>
    /// The build already makes this unrepresentable — the project does not
    /// reference Godot, so a line that reaches for it does not compile. This is
    /// here to say that was checked rather than assumed, and to fail loudly if
    /// somebody ever adds the reference.
    /// </remarks>
    [Fact]
    public void The_simulation_has_no_engine_dependency()
    {
        var referenced = typeof(World).Assembly.GetReferencedAssemblies()
            .Select(a => a.Name ?? "")
            .Where(n => n.Contains("Godot", StringComparison.OrdinalIgnoreCase))
            .ToList();

        Assert.True(
            referenced.Count == 0,
            "the simulation references the engine: " + string.Join(", ", referenced));
    }

    /// <summary>
    /// Every C# source file under <c>game/</c>, as one string.
    /// </summary>
    /// <remarks>
    /// The build output is skipped: a generated file mentioning a type is not a
    /// screen reaching it, and the source generators name every partial class in
    /// the project.
    /// </remarks>
    private static string GameSource()
    {
        var root = Path.Combine(Fixtures.RepoRoot, "game");
        var text = new System.Text.StringBuilder();

        foreach (var file in Directory.EnumerateFiles(root, "*.cs", SearchOption.AllDirectories))
        {
            if (file.Contains(".godot", StringComparison.Ordinal)
                || file.Contains($"{Path.DirectorySeparatorChar}obj{Path.DirectorySeparatorChar}",
                    StringComparison.Ordinal))
            {
                continue;
            }

            text.AppendLine(File.ReadAllText(file));
        }

        return text.ToString();
    }
}
