namespace RedRepublic.Sim;

/// <summary>One map size on offer, named in kilometres because that is what it is.</summary>
/// <remarks>
/// The archived build offered tile counts, and "128×128" told a player nothing
/// about how far a lorry would have to drive.
/// </remarks>
public readonly record struct MapSize(string Name, double Extent);

/// <summary>What the player has narrowed the shelf to.</summary>
public readonly record struct ShelfFilter(double Extent, int Climate);

/// <summary>
/// The facts a candidate card carries, beside its picture.
/// </summary>
/// <remarks>
/// Deliberately few. These are the things that decide a start, and a card that
/// listed everything would be a spreadsheet the player has to study rather than
/// a choice they can feel.
/// </remarks>
public readonly record struct CandidateStats(
    double Buildable,
    double Water,
    double Forest,
    double Coal,
    double Iron,
    double Oil,
    double Groundwater,

    /// <summary>
    /// How far the middle of the map is from workable coal, or -1 where the
    /// survey found none.
    /// </summary>
    /// <remarks>
    /// The single most decisive fact about a posting: coal under your feet is a
    /// republic, coal twelve kilometres away is a haulage problem.
    /// </remarks>
    double CoalReach,

    /// <summary>
    /// Frontier posts of each bloc. Which blocs a posting can reach, and by how
    /// many ways, is decisive: a republic with one Western post on the far side
    /// of a river trades in dollars only once it has built a road most of the way
    /// across the map.
    /// </summary>
    int CrossingsEast,
    int CrossingsWest,

    /// <summary>The coldest month's mean, in degrees Celsius — what a winter costs.</summary>
    double ColdestMonthC)
{
    /// <summary>
    /// A single rough measure of how kind a posting is, 0 to 1.
    /// </summary>
    /// <remarks>
    /// <b>For sorting and colour, not for hiding anything.</b> Every underlying
    /// figure is on the card and the player is free to disagree with the
    /// weighting. Land quality genuinely varies and nothing here guarantees a
    /// rich start — a bad hand is a legitimate hand, and papering over one would
    /// make the shelf dishonest.
    /// </remarks>
    public double Promise()
    {
        var coal = Math.Clamp(Coal / Shelf.RichCoal, 0.0, 1.0);
        var iron = Math.Clamp(Iron / Shelf.RichIron, 0.0, 1.0);
        var near = CoalReach < 0.0
            ? 0.0
            : Math.Clamp(1.0 - (CoalReach / Shelf.CoalOnYourDoorstep), 0.0, 1.0);
        var warmth = Math.Clamp((ColdestMonthC + 25.0) / 30.0, 0.0, 1.0);

        return (0.30 * Buildable) + (0.20 * coal) + (0.15 * iron)
            + (0.20 * near) + (0.15 * warmth);
    }
}

/// <summary>One posting on offer.</summary>
/// <param name="Index">Position on the shelf, stable across filter changes.</param>
/// <param name="World">
/// The land itself, generated once and kept rather than regenerated on demand.
/// <see cref="Shelf.Found"/> hands this over, so the card's honesty is true by
/// construction rather than resting on worldgen determinism holding — which is
/// an assumption nothing here was checking. It is also what the founding screen
/// renders, which needs the whole world rather than a summary.
/// </param>
public sealed record Candidate(int Index, WorldSpec Spec, CandidateStats Stats, World World);

/// <summary>
/// Choosing your land: the shelf of candidate postings.
/// </summary>
/// <remarks>
/// <para>
/// Founding is not a dialog with a seed box. It is <b>choosing your land</b>, and
/// the first beat of it is a shelf of candidate maps derived from one master
/// seed, each shown with the few facts that actually decide a start. Moscow
/// surveyed the ground before assigning the posting, so the player sees what the
/// survey saw — no more, and no less.
/// </para>
/// <para>
/// <b>Filters transform the seeds, they do not replace them.</b> Changing size or
/// climate keeps the same candidate seeds and re-derives them under the new
/// filter. That is the whole reason the shelf is worth building: flipping from
/// plains to taiga shows what that does to land you were already considering,
/// rather than dealing you six strangers. Candidate <i>n</i> is always candidate
/// <i>n</i>.
/// </para>
/// <para>
/// <b>The stats come from the engine, never from here.</b> Every figure on a card
/// is read from the geology and the terrain, which are the same sources the
/// in-game survey overlay reads. A shelf that computed its own idea of
/// "coal-rich" would eventually disagree with the game it is advertising, and the
/// player would be right to call that a lie.
/// </para>
/// <para>
/// <b>It is not free.</b> Generating a candidate costs a full worldgen, so a
/// shelf costs six, and a re-filter costs six more because a filter genuinely
/// re-derives the land. That needs a loading state at every size.
/// </para>
/// </remarks>
public sealed class Shelf
{
    /// <summary>
    /// Substream identifier for the shelf.
    /// </summary>
    /// <remarks>
    /// Its own stream, so the shelf a master seed produces never shifts because
    /// something else about worldgen changed.
    /// </remarks>
    public const ulong Stream = 11;

    /// <summary>
    /// How many candidates a shelf offers.
    /// </summary>
    /// <remarks>
    /// Six: enough that the choice is real, few enough that every card can carry
    /// its picture and its numbers without a scrollbar.
    /// </remarks>
    public const int Size = 6;

    // The three figures the promise score is measured against. They are
    // presentation rather than economy — the score sorts and colours the cards
    // and hides nothing, because every underlying figure is on the card — so
    // they are authored here beside the thing that reads them rather than in the
    // balance table.

    /// <summary>Tonnes of coal a survey has to find for a posting to read as rich.</summary>
    public const double RichCoal = 5_000_000.0;

    /// <summary>The same for iron.</summary>
    public const double RichIron = 3_000_000.0;

    /// <summary>Beyond this, coal is a haulage problem rather than a seam.</summary>
    public const double CoalOnYourDoorstep = 5_000.0;

    /// <summary>The map sizes on offer, smallest first.</summary>
    public static IReadOnlyList<MapSize> Sizes { get; } =
    [
        new("Small", 6_000.0),
        new("Standard", 10_000.0),
        new("Large", 16_000.0),
    ];

    private Shelf(ulong masterSeed, ShelfFilter filter, IReadOnlyList<Candidate> candidates)
    {
        MasterSeed = masterSeed;
        Filter = filter;
        Candidates = candidates;
    }

    public ulong MasterSeed { get; }

    public ShelfFilter Filter { get; }

    public IReadOnlyList<Candidate> Candidates { get; }

    /// <summary>
    /// The seed of one candidate under a master seed.
    /// </summary>
    /// <remarks>
    /// A pure function of the two, and deliberately independent of the filter:
    /// that is what makes "the same land under a different sky" possible.
    /// </remarks>
    public static ulong CandidateSeed(ulong masterSeed, int index) =>
        Rng.Derive(masterSeed, Stream) ^ ((ulong)index * 0x9E37_79B9_7F4A_7C15UL);

    /// <summary>Generate a shelf. One full worldgen per candidate.</summary>
    public static Shelf Derive(ulong masterSeed, ShelfFilter filter, Tables t)
    {
        ArgumentNullException.ThrowIfNull(t);
        var candidates = new List<Candidate>(Size);

        for (var index = 0; index < Size; index++)
        {
            var spec = new WorldSpec(
                CandidateSeed(masterSeed, index), filter.Extent, filter.Climate);
            var world = World.Found(spec, t);
            candidates.Add(new Candidate(index, spec, Survey(world), world));
        }

        return new Shelf(masterSeed, filter, candidates);
    }

    /// <summary>The same shelf under a different filter — same seeds, new land.</summary>
    public Shelf Refilter(ShelfFilter filter, Tables t) => Derive(MasterSeed, filter, t);

    public Candidate? Get(int index) =>
        index >= 0 && index < Candidates.Count ? Candidates[index] : null;

    /// <summary>
    /// The world for a candidate — the one the player actually plays.
    /// </summary>
    /// <remarks>
    /// The land the card described, not a fresh world built from the same spec.
    /// Those two are supposed to be identical, and the difference still matters:
    /// regenerating would make the card's honesty depend on determinism holding,
    /// which is an assumption nothing was checking.
    /// </remarks>
    public World? Found(int index) => Get(index)?.World;

    /// <summary>Read a generated world the way the in-game survey overlay would.</summary>
    public static CandidateStats Survey(World world)
    {
        ArgumentNullException.ThrowIfNull(world);
        var water = world.Terrain.FractionOf(Surface.Water);
        var middle = world.Terrain.Extent / 2.0;

        var east = 0;
        var west = 0;
        foreach (var crossing in world.Frontier.Crossings)
        {
            if (crossing.Bloc == Market.East)
            {
                east++;
            }
            else
            {
                west++;
            }
        }

        return new CandidateStats(
            Buildable: 1.0 - water,
            Water: water,
            Forest: world.Terrain.FractionOf(Surface.Forest),
            Coal: world.Geology.RemainingOf(Mineral.Coal),
            Iron: world.Geology.RemainingOf(Mineral.IronOre),
            Oil: world.Geology.RemainingOf(Mineral.Oil),
            Groundwater: world.Geology.RemainingOf(Mineral.Groundwater),
            CoalReach: world.Geology.DistanceToNearest(middle, middle, Mineral.Coal) ?? -1.0,
            CrossingsEast: east,
            CrossingsWest: west,
            ColdestMonthC: world.Climate.ColdestMeanC);
    }
}
