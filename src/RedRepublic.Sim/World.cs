namespace RedRepublic.Sim;

/// <summary>What a republic is founded on.</summary>
public readonly record struct WorldSpec(ulong Seed, double Extent, int Climate);

/// <summary>
/// One republic: everything the simulation knows.
/// </summary>
/// <remarks>
/// <para>
/// <b>The player's surface is two verbs, <see cref="Tick"/> and
/// <see cref="Issue"/>.</b> Everything here is read-only from outside; the only
/// ways a republic changes are advancing time and asking for something. A
/// refusal returns a sentence a screen can show, worded beside the reason.
/// </para>
/// <para>
/// <b>Systems propose, one writer applies.</b> No system reaches into this
/// directly — each returns the changes it wants and <see cref="Systems.Apply"/>
/// is the only thing that carries them out. That is what makes "which system
/// wrote this?" answerable, and it is why each system declares what it may
/// change and the declaration is checked in both directions.
/// </para>
/// </remarks>
public sealed class World
{
    private readonly List<RoadSite> _roadbook = [];

    private World(WorldSpec spec, Tables tables)
    {
        Spec = spec;
        Tables = tables;
        Clock = new SimClock();
        Rng = Rng.FromSeed(spec.Seed);
        Buildings = new Buildings(tables);
        Citizens = new Citizens(tables);
        Fleet = new Fleet(tables);
        RoadWorks = new RoadWorks(tables);
        LineWorks = new LineWorks(tables);
        Grid = new Networks(tables);
        Crews = new Crews();
        Migration = new Migration();
        Tourism = new Tourism();
        Treasury = new Treasury();
        Loans = new Loans(tables);
        Contracts = new Contracts(tables);
        BuildPolicy = new BuildPolicy();
        Journal = new Journal();
        TradeRules = [];
        Ground = new Ground();
        Name = "";

        Terrain = Terrain.Generate(spec.Seed, spec.Extent, tables);

        // Geology draws on its own stream, so changing the terrain plan does not
        // re-roll what is under the ground.
        Geology = Geology.Generate(spec.Seed + tables.GeologyStream, spec.Extent, tables);
        Lattice = Lattice.FromTerrain(Terrain, tables);
        Frontier = Frontier.Generate(spec.Extent, Rng.FromSeed(spec.Seed + 2), tables);

        Roads = new Network();
        Rails = new Network();
        Tramway = new Network();
        Metro = new Network();
        Waterways = Network.Navigable(Terrain, tables);
        Airways = new Network();
    }

    public WorldSpec Spec { get; }

    public Tables Tables { get; }

    public SimClock Clock { get; }

    /// <summary>
    /// The running simulation's generator. Its stream position goes into the
    /// save, because a save that restores the seed but not the position resumes
    /// a different future.
    /// </summary>
    public Rng Rng { get; }

    public Terrain Terrain { get; }

    public Geology Geology { get; }

    public Lattice Lattice { get; }

    public Ground Ground { get; internal set; }

    public Buildings Buildings { get; }

    public Citizens Citizens { get; }

    public Fleet Fleet { get; }

    public RoadWorks RoadWorks { get; }

    /// <summary>
    /// Every run the republic has opened, in the order it opened them.
    /// </summary>
    /// <remarks>
    /// The network itself is not saved: junction merging depends on what already
    /// stood there, so replaying the openings in order reproduces the graph
    /// exactly while storing the graph would lose how it came to be. This is that
    /// record, and <see cref="Reopen"/> is the only thing that adds to it.
    /// </remarks>
    public IReadOnlyList<RoadSite> Roadbook => _roadbook;

    public LineWorks LineWorks { get; }

    /// <summary>
    /// The energised lines, and who is plugged into them. What makes a power
    /// station a thing that stands somewhere rather than a number.
    /// </summary>
    public Networks Grid { get; }

    public Crews Crews { get; }

    public Migration Migration { get; }

    public Tourism Tourism { get; }

    public Frontier Frontier { get; }

    public Treasury Treasury { get; }

    public Loans Loans { get; }

    public Contracts Contracts { get; }

    public BuildPolicy BuildPolicy { get; }

    /// <summary>
    /// The standing instructions to the customs houses, in the order they are
    /// served. <b>The order is the decision</b>: when throughput or money runs
    /// short the first rule is served first.
    /// </summary>
    public List<TradeRule> TradeRules { get; }

    public Journal Journal { get; }

    // ---- the six networks ----
    //
    // Six instances of one graph type rather than six types. A train routes over
    // the rails and finds no road there, because the road is somewhere else.
    public Network Roads { get; }

    public Network Rails { get; }

    public Network Tramway { get; }

    public Network Metro { get; }

    /// <summary>Not built — sampled off the terrain, because nobody digs a river.</summary>
    public Network Waterways { get; }

    public Network Airways { get; }

    public Climate Climate => Tables.Climates[Spec.Climate];

    /// <summary>
    /// What the player called it. An input like any other, so it is journalled
    /// and a replayed republic comes back called what the player called it.
    /// </summary>
    public string Name { get; internal set; }

    public static World Found(WorldSpec spec, Tables tables)
    {
        ArgumentNullException.ThrowIfNull(tables);
        return new World(spec, tables);
    }

    /// <summary>
    /// Lay a finished run into the network its grade carries, and record that it
    /// was laid.
    /// </summary>
    /// <remarks>
    /// The single path a road takes into the graph, so the book and the network
    /// cannot disagree — a run opened round the back of this would vanish on the
    /// next reload, and the republic would come back with a hole in its roads
    /// that nothing had reported.
    /// </remarks>
    internal void Reopen(RoadSite run)
    {
        RoadWorks.Open(NetworkFor(Tables.Grades[run.Grade].Carries), run, Tables);
        _roadbook.Add(run);
    }

    /// <summary>The network a medium travels on.</summary>
    public Network NetworkFor(Medium medium) => medium switch
    {
        Medium.Road => Roads,
        Medium.Rail => Rails,
        Medium.Tram => Tramway,
        Medium.Metro => Metro,
        Medium.Water => Waterways,
        _ => Airways,
    };

    /// <summary>
    /// Advance one tick — the only way time moves.
    /// </summary>
    public IReadOnlyList<Mutation> Tick() => Systems.RunTick(this);

    /// <summary>
    /// Carry out a player command, or say why not.
    /// </summary>
    /// <remarks>
    /// An accepted command is recorded in the journal as it is applied; a
    /// refused one changes nothing and is not recorded, because replaying a
    /// no-op is not replay. That recording is what gives the determinism rule's
    /// <i>same seed and same inputs</i> half an <b>inputs</b> to hold constant.
    /// </remarks>
    public Outcome Issue(Command command)
    {
        ArgumentNullException.ThrowIfNull(command);
        var outcome = Commands.CarryOut(this, command);
        if (outcome.Accepted)
        {
            Journal.Record(Clock.Ticks, command);
        }

        return outcome;
    }
}

/// <summary>
/// Every accepted command, in the order it was issued.
/// </summary>
/// <remarks>
/// A save records how its republic came to be, which is what makes a replay
/// possible at all. Refused commands are not recorded: replaying a no-op is not
/// replay.
/// </remarks>
public sealed class Journal
{
    private readonly List<(long Tick, Command Command)> _entries = [];

    public int Count => _entries.Count;

    public IReadOnlyList<(long Tick, Command Command)> Entries => _entries;

    public void Record(long tick, Command command) => _entries.Add((tick, command));
}
