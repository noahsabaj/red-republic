using System.Reflection;
using System.Text.RegularExpressions;

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
    /// Verbs a player has and no screen offers yet.
    /// </summary>
    /// <remarks>
    /// The same kind of work list as <see cref="NotYetReached"/>, meant to reach
    /// nothing for the same reason.
    /// </remarks>
    private static readonly string[] NoControlYet = [];

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
            if (Reaches(game, fact.Name))
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
        var stale = NotYetReached.Where(name => Reaches(game, name)).ToList();

        Assert.True(
            stale.Count == 0,
            "these are on the work list and are reached after all: "
            + string.Join(", ", stale)
            + " — delete their lines");
    }

    /// <summary>
    /// Every verb a player has is on a screen, or is on the work list saying it
    /// is not.
    /// </summary>
    /// <remarks>
    /// <para>
    /// <b>The other half of condition one, and nothing guarded it.</b> This file
    /// checked the nouns — what a republic <i>is</i> — and never the verbs, so
    /// thirteen of the twenty-three things a player can ask for reached no
    /// control anywhere, including the two the founding's own remarks name as
    /// the opening: contract a Bloc firm, and hire foreign workers. The journal
    /// narrated orders the player could not give.
    /// </para>
    /// <para>
    /// Which factory makes which verb is derived rather than listed. A
    /// hand-written map is a second place for the answer to live, and the one
    /// that rots: a verb renamed would quietly stop being checked.
    /// </para>
    /// </remarks>
    [Fact]
    public void Every_verb_a_player_has_reaches_a_control_or_is_on_the_work_list()
    {
        var game = GameSource();
        var unreached = new List<string>();

        foreach (var kind in Enum.GetValues<CommandKind>())
        {
            if (!Offered(game, kind))
            {
                unreached.Add(kind.ToString());
            }
        }

        var missing = unreached.Except(NoControlYet).ToList();
        Assert.True(
            missing.Count == 0,
            "a player can ask for these and no screen offers them: "
            + string.Join(", ", missing)
            + " — either wire one up, or put it on NoControlYet with a line saying why");
    }

    /// <summary>
    /// Every verb the journal can be asked to narrate has a sentence.
    /// </summary>
    /// <remarks>
    /// <b>A save records how its republic came to be</b>, and the journal screen
    /// is where a player reads it. Its switch ends in a fallback saying it
    /// cannot name what happened — which is a page of history that says nothing,
    /// and which nothing would have reported: a verb added without a sentence
    /// simply started producing it.
    /// </remarks>
    [Fact]
    public void Every_verb_the_journal_can_be_handed_has_a_sentence()
    {
        var game = GameSource();
        var unnamed = Enum.GetValues<CommandKind>()
            .Where(kind => !game.Contains($"CommandKind.{kind} =>", StringComparison.Ordinal))
            .Select(kind => kind.ToString())
            .ToList();

        Assert.True(
            unnamed.Count == 0,
            "the journal has no sentence for these and would print its fallback: "
            + string.Join(", ", unnamed));
    }

    /// <summary>The verb work list only shrinks, for the same reason.</summary>
    [Fact]
    public void The_verb_work_list_names_nothing_that_is_already_offered()
    {
        var game = GameSource();
        var stale = NoControlYet
            .Where(name => Offered(game, Enum.Parse<CommandKind>(name)))
            .ToList();

        Assert.True(
            stale.Count == 0,
            "these are on the verb work list and are offered after all: "
            + string.Join(", ", stale) + " — delete their lines");
    }

    /// <summary>Whether any screen issues a verb.</summary>
    private static bool Offered(string game, CommandKind kind)
    {
        foreach (var factory in FactoriesFor(kind))
        {
            if (game.Contains($"Command.{factory}(", StringComparison.Ordinal))
            {
                return true;
            }
        }

        return false;
    }

    /// <summary>
    /// Which of <see cref="Command"/>'s named constructors produce a verb.
    /// </summary>
    /// <remarks>
    /// Found by building one of each with default arguments and reading what
    /// came out — the only way to be sure the answer stays true when a
    /// constructor is renamed. Safe to call them all: a named constructor fills
    /// in fields and refuses nothing, because refusal is
    /// <see cref="Commands.CarryOut"/>'s business and needs a republic.
    /// </remarks>
    private static List<string> FactoriesFor(CommandKind kind)
    {
        var made = new List<string>();
        foreach (var factory in typeof(Command).GetMethods(BindingFlags.Public | BindingFlags.Static))
        {
            if (factory.ReturnType != typeof(Command))
            {
                continue;
            }

            var arguments = factory.GetParameters().Select(Blank).ToArray();
            if (factory.Invoke(null, arguments) is Command command && command.Kind == kind)
            {
                made.Add(factory.Name);
            }
        }

        return made;
    }

    private static object? Blank(ParameterInfo p)
    {
        if (p.ParameterType == typeof(string))
        {
            return "";
        }

        return p.ParameterType.IsValueType && Nullable.GetUnderlyingType(p.ParameterType) is null
            ? Activator.CreateInstance(p.ParameterType)
            : null;
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
    /// Whether a screen reads a fact <i>off a republic</i>.
    /// </summary>
    /// <remarks>
    /// <b>The receiver is the whole check.</b> This used to look for the name
    /// after a dot anywhere in the game's source, which meant every fact whose
    /// name is an ordinary word was permanently satisfied by something else
    /// entirely: <c>Name</c> by <c>node.Name</c>, <c>Grid</c> by a container,
    /// <c>Clock</c> by anything at all. Deleting the screen that showed one
    /// would not have failed this guard — a guard passing without reaching its
    /// subject, in the file whose whole job is to notice that.
    /// <para>
    /// The three receivers are the three ways the game holds a republic: the
    /// root's own field, a local, and the property every screen inherits.
    /// </para>
    /// </remarks>
    private static bool Reaches(string game, string fact) =>
        Regex.IsMatch(game, $@"\b(_?world|Republic)\.{Regex.Escape(fact)}\b");

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
