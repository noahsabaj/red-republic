namespace RedRepublic.Sim;

/// <summary>What a founding managed to put down.</summary>
public sealed class StartingBase
{
    public List<int> Housing { get; } = [];

    public List<int> Substations { get; } = [];

    public int Mine { get; internal set; } = -1;

    public int Plant { get; internal set; } = -1;

    public int Woodcutter { get; internal set; } = -1;

    public int Sawmill { get; internal set; } = -1;

    public int Store { get; internal set; } = -1;

    public int ConstructionOffice { get; internal set; } = -1;

    public int Depot { get; internal set; } = -1;

    /// <summary>The garage the republic's lorries live in. Without it nothing moves.</summary>
    public int MotorDepot { get; internal set; } = -1;

    public int Farm { get; internal set; } = -1;

    public int FoodFactory { get; internal set; } = -1;

    public int TextileMill { get; internal set; } = -1;

    public int Boiler { get; internal set; } = -1;

    /// <summary>-1 when the border could not take a crossing on this seed.</summary>
    public int Customs { get; internal set; } = -1;

    /// <summary>Where the town centre ended up.</summary>
    public double CentreX { get; internal set; }

    public double CentreY { get; internal set; }
}

/// <summary>
/// Founding a republic that can actually run.
/// </summary>
/// <remarks>
/// A scripted opening that the trajectory runner and the balance tests both use,
/// so a change to the balance is measured against the same republic every time.
/// It is deliberately not clever: it sites a town on real generated ground and
/// reports what it managed to place. A seed whose coal is under a lake is a
/// legitimate outcome and the caller should see it, not have it papered over.
/// </remarks>
public static class Scenario
{
    /// <summary>
    /// The grant a posting opens with, in roubles — read from the table, which
    /// is where balance lives.
    /// </summary>
    /// <remarks>
    /// <b>Roubles only, and that is the opening move.</b> A republic starts
    /// inside the Eastern Bloc's economy: it can hire Bloc contractors and buy
    /// Bloc goods from day one, and everything Western — machinery, contractors,
    /// the better prices — is out of reach until it has exported something for
    /// hard currency. Which bloc's posts the land actually reaches is therefore
    /// the first thing that matters about a posting, rather than a detail that
    /// surfaces hours in.
    /// </remarks>
    public static double GrantRoubles(Tables tables) =>
        (tables ?? throw new ArgumentNullException(nameof(tables))).OpeningGrant;

    /// <summary>
    /// The population a <i>test</i> town is stood up with.
    /// </summary>
    /// <remarks>
    /// <b>A fixture number, not a game fact.</b> Nothing is given to a real
    /// republic — see <see cref="Found"/>, which places nothing at all. It stays
    /// because the tests and the trajectory runner need a working town to measure
    /// against, and it is exactly enough to staff what <see cref="Town"/> builds.
    /// </remarks>
    public const int Settlers = 143;

    /// <summary>
    /// Take a posting: pick the site, open the grant, and hand over an empty map.
    /// </summary>
    /// <remarks>
    /// <para>
    /// <b>Nothing is built and nobody lives here.</b> A republic is a blank slate
    /// — the land, a rouble balance, and whatever the player does next. That is
    /// how the game this one is measured against works, and it is what makes the
    /// opening a plan rather than an inheritance.
    /// </para>
    /// <para>
    /// The site is still chosen from the geology, because a posting is assigned
    /// where there is something worth mining, and the camera has to open
    /// somewhere. The bootstrap uses only machinery that already exists: contract
    /// a Bloc firm to build a Construction Office, hire foreign workers into it,
    /// and from there the republic builds itself.
    /// </para>
    /// </remarks>
    public static (double X, double Y) Found(World world)
    {
        ArgumentNullException.ThrowIfNull(world);
        var centre = Site(world);
        world.Treasury.Add(Market.East, GrantRoubles(world.Tables));
        return centre;
    }

    /// <summary>
    /// Where a posting is centred: the shallowest coal body, or the middle of the
    /// map if the survey found none.
    /// </summary>
    public static (double X, double Y) Site(World world)
    {
        ArgumentNullException.ThrowIfNull(world);
        Deposit? shallowest = null;

        foreach (var deposit in world.Geology.All)
        {
            if (deposit.Mineral != Mineral.Coal || deposit.IsExhausted)
            {
                continue;
            }

            // Strictly shallower, so the first body in the survey's own order
            // takes a tie and two runs of the same seed centre on the same one.
            if (shallowest is null || deposit.Top < shallowest.Top)
            {
                shallowest = deposit;
            }
        }

        if (shallowest is null)
        {
            var middle = world.Terrain.Extent / 2.0;
            return (middle, middle);
        }

        return (shallowest.CentreX, shallowest.CentreY);
    }

    /// <summary>
    /// Search outward from a point for somewhere this kind of building will
    /// stand, or -1.
    /// </summary>
    /// <remarks>
    /// A square spiral on the terrain lattice: deterministic, and it finds the
    /// nearest workable site rather than the first one a random search stumbles
    /// into.
    /// </remarks>
    public static (double X, double Y)? FindSite(
        World world, int kind, double nearX, double nearY, double within)
    {
        ArgumentNullException.ThrowIfNull(world);

        // Two cells at whatever resolution this map was generated at, so the
        // search stays proportionate to the ground it is searching.
        var step = world.Terrain.CellSize * 2.0;
        var rings = (int)Math.Ceiling(within / step);

        for (var ring = 0; ring <= rings; ring++)
        {
            for (var dy = -ring; dy <= ring; dy++)
            {
                for (var dx = -ring; dx <= ring; dx++)
                {
                    // Only the perimeter of this ring — the inside was searched
                    // on an earlier pass.
                    if (ring > 0 && Math.Abs(dx) != ring && Math.Abs(dy) != ring)
                    {
                        continue;
                    }

                    var x = nearX + (dx * step);
                    var y = nearY + (dy * step);
                    if (Commands.CanPlace(world, kind, x, y) is null)
                    {
                        return (x, y);
                    }
                }
            }
        }

        return null;
    }

    /// <summary>
    /// Stand up a working town, for tests and for the trajectory runner.
    /// </summary>
    /// <remarks>
    /// <para>
    /// <b>This is a fixture, not the game's founding.</b> <see cref="Found"/>
    /// places nothing; a real republic builds every one of these itself. It
    /// exists because a great many tests need a republic that already works in
    /// order to measure one system, and standing that up through the player's own
    /// verbs in each of them would be slower to run and no more honest.
    /// </para>
    /// <para>
    /// <b>The order below is a priority order.</b> Labour fills workplaces in
    /// commissioning order, so whatever is founded last is what goes unmanned
    /// when the republic is short of people — and the tail is the customs house,
    /// which is the difference between a hard start and a republic that can never
    /// earn a rouble.
    /// </para>
    /// <para>
    /// <b>Moscow sends a working town, wired.</b> A plant lights only what is
    /// strung to it and a boiler warms only what a main runs past, so a founding
    /// that placed the buildings and no lines would arrive dark and cold. That is
    /// not a hard start; it is half the game switched off.
    /// </para>
    /// </remarks>
    public static StartingBase Town(World world, int citizens)
    {
        ArgumentNullException.ThrowIfNull(world);
        var t = world.Tables;
        var (cx, cy) = Site(world);
        var walk = t.MaxWalkM / 2.0;

        var b = new StartingBase { CentreX = cx, CentreY = cy };

        b.Mine = Put(world, "CoalMine", cx, cy, 400.0);

        // Housing close enough to walk to everything.
        for (var i = 0; i < 3; i++)
        {
            var block = Put(world, "Apartment", cx + 300.0 + (i * 120.0), cy, 600.0);
            if (block >= 0)
            {
                b.Housing.Add(block);
            }
        }

        b.Plant = Put(world, "PowerPlant", cx, cy + 500.0, 800.0);

        // The boiler house goes up with the power station, not after the shops:
        // heat and power are the two life-support systems, and a boiler sited
        // last is the first thing to go unmanned when a republic is short of
        // people — an under-populated founding then freezes while its textile
        // mill runs.
        b.Boiler = Put(world, "HeatingPlant", cx + 200.0, cy - 450.0, walk);

        // Three transformer stations rather than one, because a station serves a
        // radius and the founding is spread over about a kilometre. They are what
        // the consumers plug into; the lines only join the stations to the plant.
        foreach (var (sx, sy) in new[] { (cx - 150.0, cy + 180.0), (cx + 400.0, cy + 150.0), (cx - 50.0, cy - 400.0) })
        {
            var station = Put(world, "TransformerStation", sx, sy, 250.0);
            if (station >= 0)
            {
                b.Substations.Add(station);
            }
        }

        // From the plant to the first station, then station to station. Every
        // span is energised on the spot: the founding hand arrives finished,
        // exactly as its buildings do.
        if (b.Plant >= 0)
        {
            var previous = b.Plant;
            foreach (var station in b.Substations)
            {
                StringUp(world, "Power", previous, station);
                previous = station;
            }
        }

        // And the heat main, chained boiler to block to block. A main reaches
        // barely a hundred metres sideways, so it has to run <i>to</i> each block
        // rather than past the row — which is the whole reason district heating
        // is a town-scale thing and a remote camp wants its own boiler.
        if (b.Boiler >= 0)
        {
            var previous = b.Boiler;
            foreach (var block in b.Housing)
            {
                StringUp(world, "Heat", previous, block);
                previous = block;
            }
        }

        // And haulage, in the same breath and for the same reason. Freight is
        // physical: without a garage and its lorries nothing reaches anything and
        // the republic starves beside its own full bins. That makes the motor
        // depot life-support, and life-support goes in before the shops.
        b.MotorDepot = Put(world, "MotorDepot", cx - 300.0, cy - 500.0, walk);
        b.Woodcutter = Put(world, "Woodcutter", cx - 600.0, cy, walk);
        b.Sawmill = Put(world, "Sawmill", cx - 400.0, cy, walk);
        b.Store = Put(world, "Store", cx + 250.0, cy + 150.0, 400.0);
        b.ConstructionOffice = Put(world, "ConstructionOffice", cx, cy - 300.0, 600.0);
        b.Depot = Put(world, "Depot", cx + 150.0, cy - 200.0, 600.0);

        // The food chain. Without it the opening grant is eaten in two months and
        // the republic starves for ever after — which is exactly what the
        // trajectory runner showed before this was here.
        b.Farm = Put(world, "Farm", cx, cy + 900.0, walk);
        b.FoodFactory = Put(world, "FoodFactory", cx + 400.0, cy + 400.0, walk);

        // And clothes. Without it the republic sits at 79% provisioned for ever —
        // fed but never clothed.
        b.TextileMill = Put(world, "TextileMill", cx - 200.0, cy + 400.0, walk);

        // A crossing on whatever edge is foreign. This does not look for
        // somewhere to put one — it picks which existing post the republic opens
        // a house at, which is the nearest, because the founding grant is one
        // house and the haul to it is the republic's first long journey.
        var post = world.Frontier.NearestCrossing(cx, cy, null);
        if (post is not null)
        {
            b.Customs = PlaceBuilt(world, t.BuildingIndex("Customs"), post.Value.X, post.Value.Y);
        }

        // Moscow's opening grant. Coal to light the plant before the mine
        // produces, and food and clothes so the first winter is not immediate
        // starvation. Everything domestic — no currency, per the border rule.
        Give(world, b.Plant, "Coal", 60.0);
        Give(world, b.Depot, "Food", 120.0);
        Give(world, b.Depot, "Clothes", 40.0);
        Give(world, b.Depot, "Planks", 80.0);
        Give(world, b.Depot, "Bricks", 80.0);

        // Diesel, split between the two garages in their own tanks — and that
        // split is forced by a rule in freight: a supplier is anybody holding a
        // resource who does not <i>consume</i> it. Both yards burn diesel, so
        // neither can pass any to the other, and whichever one the grant landed
        // in would be the only one with any. Moscow fuels the vehicles it sends.
        Give(world, b.MotorDepot, "Fuel", 14.0);
        Give(world, b.Depot, "Fuel", 6.0);

        // Settlers, spread evenly over what housing exists, and across working
        // life so the republic is not a single cohort that retires at once.
        if (b.Housing.Count > 0)
        {
            for (var i = 0; i < citizens; i++)
            {
                var home = b.Housing[i % b.Housing.Count];
                world.Citizens.AddArrival(world.Buildings.IdAt(home), 18 + (i % 40));
            }
        }

        return b;
    }

    /// <summary>Find ground for a kind near a point and stand it up finished.</summary>
    private static int Put(World world, string id, double x, double y, double reach)
    {
        var kind = world.Tables.BuildingIndex(id);
        var at = FindSite(world, kind, x, y, reach);
        return at is null ? -1 : PlaceBuilt(world, kind, at.Value.X, at.Value.Y);
    }

    /// <summary>
    /// Place a building already finished — the founding grant, and nothing else.
    /// </summary>
    /// <remarks>
    /// There is no player action that makes a finished building appear, which is
    /// why this is not a command. The grant is scenario setup rather than play.
    /// </remarks>
    private static int PlaceBuilt(World world, int kind, double x, double y)
    {
        var placed = world.Issue(Command.Place(kind, x, y));
        if (!placed.Accepted)
        {
            return -1;
        }

        var i = world.Buildings.IndexOf(placed.Id);
        world.Buildings.AddWork(i, world.Tables.BLabour[kind]);
        return i;
    }

    /// <summary>
    /// Order a span between two buildings and energise it there and then.
    /// </summary>
    /// <remarks>
    /// The founding grant arrives finished — buildings and grid alike — so this
    /// goes round the construction queue exactly as the buildings do. Every other
    /// span in the republic's life is ordered, materialled and strung by a crew.
    /// </remarks>
    private static void StringUp(World world, string utility, int from, int to)
    {
        if (from < 0 || to < 0)
        {
            return;
        }

        var kind = world.Tables.UtilityIndex(utility);
        var refusal = world.LineWorks.Order(
            kind,
            world.Buildings.XAt(from), world.Buildings.YAt(from),
            world.Buildings.XAt(to), world.Buildings.YAt(to),
            world.Buildings.Commissioned, out var site);

        if (refusal != LineError.None || site is null)
        {
            return; // shorter than a span worth surveying
        }

        world.LineWorks.Finish(site);
        world.Grid.Energise(site);
        world.Grid.AttachAlong(world.Buildings, kind);
    }

    private static void Give(World world, int building, string resource, double tonnes)
    {
        if (building >= 0)
        {
            world.Buildings.Stock.Add(building, world.Tables.ResourceIndex(resource), tonnes);
        }
    }
}
