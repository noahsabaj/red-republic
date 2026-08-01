namespace RedRepublic.Sim.Tests;

/// <summary>
/// A republic driven into every corner it has.
/// </summary>
/// <remarks>
/// <para>
/// <b>What the write-set guard needs and an ordinary fixture cannot give it.</b>
/// A declaration is only claiming too much if <i>nothing the republic can do</i>
/// reaches it, so the check has to be made against a republic that does
/// everything: hires builders abroad, defaults on an advance, fails a tender,
/// trades in both directions, has a road and a span under construction, keeps a
/// hotel, and lives through a winter so the ploughs have something to clear.
/// </para>
/// <para>
/// It is deliberately not a <i>good</i> republic. Nothing here is balance and
/// none of it is what a player would do; it is a list of the corners, and the
/// only thing asserted against it is which systems reached which kinds.
/// </para>
/// </remarks>
public static class Busy
{
    /// <summary>
    /// How the fixture spends its time: days skipped, then days lived.
    /// </summary>
    /// <remarks>
    /// <b>Winter is nine months from a founding and the gate runs in seconds.</b>
    /// Simulating every one of those days is four hundred thousand ticks of a
    /// republic nobody is watching, so the clock is wound on between the two
    /// stretches that matter: a summer long enough for the building programme,
    /// the tenders and the advance, and a December long enough for the snow to
    /// lie and the ploughs to go out. Winding on is not simulating — nothing
    /// happens in the skipped months, and nothing in the guard depends on
    /// anything having happened in them.
    /// </remarks>
    public static IReadOnlyList<(int Skip, int Live)> Schedule { get; } =
    [
        (0, 90),
        (215, 70),
    ];

    private static Tables T => Fixtures.Tables;

    /// <summary>
    /// Stand the republic up, and say what it managed — a seed whose coal is
    /// under a lake is a legitimate outcome and the caller should see it.
    /// </summary>
    public static World Republic(out string report)
    {
        // The taiga, because snow is one of the corners.
        var world = World.Found(new WorldSpec(1961, 3000.0, 1), T);
        var built = Scenario.Town(world, Scenario.Settlers);
        var notes = new List<string>();

        world.Treasury.Add(Market.East, Scenario.GrantRoubles);
        world.Treasury.Add(Market.West, Scenario.GrantRoubles);

        // Materials for the sites, and machinery for the gangs. A republic that
        // cannot feed its own building programme reaches none of the passes
        // that exist to run it.
        if (built.Depot >= 0)
        {
            foreach (var (id, tonnes) in new[]
            {
                ("Steel", 400.0), ("Gravel", 400.0), ("Machinery", 60.0),
                ("Asphalt", 200.0), ("Bricks", 200.0), ("Planks", 200.0),
            })
            {
                world.Buildings.Stock.Add(built.Depot, T.ResourceIndex(id), tonnes);
            }
        }

        if (built.ConstructionOffice >= 0)
        {
            world.Buildings.Stock.Add(
                built.ConstructionOffice, T.ResourceIndex("Machinery"), 40.0);
            world.Buildings.Stock.Add(built.ConstructionOffice, T.ResourceIndex("Fuel"), 40.0);
        }

        if (built.MotorDepot >= 0)
        {
            world.Buildings.Stock.Add(built.MotorDepot, T.ResourceIndex("Fuel"), 60.0);
        }

        // The institutions, so contentment has every want to answer and the
        // services layer is exercised rather than merely authored.
        foreach (var id in new[]
        {
            "Clinic", "School", "CultureClub", "FireStation", "Store",
            "Landfill", "Hotel", "BusDepot", "GasStation",
        })
        {
            var stood = Stand(world, id, built.CentreX, built.CentreY);
            if (stood < 0)
            {
                notes.Add($"nowhere near the centre would take a {id}");
                continue;
            }

            // First in the labour plan, so the fixture is about what the
            // systems reach rather than about whether a republic of a hundred
            // and forty-three happened to have ten people left over for a
            // school.
            world.Issue(Command.SetPriority(world.Buildings.IdAt(stood), Priority.First));

            // A depot with no diesel sends no bus, and that is the rule rather
            // than a gap — so the fixture fuels the ones it stands up.
            if (T.Inputs.LengthOf(world.Buildings.KindAt(stood)) > 0)
            {
                world.Buildings.Stock.Add(stood, T.ResourceIndex("Fuel"), 40.0);
            }
        }

        // A works nobody can walk to, and the housing stays where it is: this
        // is the only way a commute becomes a seat on something, and the seat
        // is what the labour pass burns diesel for.
        var dirt = Grade("Dirt");
        var paved = Grade("Paved");

        // The furthest corner a way will actually reach. Tried rather than
        // assumed: a straight run to the far side of a generated map crosses
        // open water as often as not, and a road is not a bridge.
        var far = Outpost(world, built.CentreX, built.CentreY, dirt, out var reached);
        if (!reached)
        {
            notes.Add("no corner of this map could be reached by road");
        }
        var pit = Stand(world, "Woodcutter", far.X, far.Y);
        if (pit < 0)
        {
            notes.Add("nowhere far enough out would take a workplace");
        }
        else
        {
            world.Issue(Command.SetPriority(world.Buildings.IdAt(pit), Priority.First));
        }

        // More estates, and more people in most of them. The founding fills its
        // own blocks to the roof and staffs about as many jobs as it has hands,
        // so a fixture that adds a hotel and a fire station runs out of people
        // before it reaches the bus depot — and an unstaffed depot runs no
        // service, which is the one thing a commute needs. The last block is
        // left empty on purpose: an intake needs somewhere to go.
        var blocks = new List<int>();
        for (var i = 0; i < 4; i++)
        {
            var block = Stand(world, "Apartment", built.CentreX - 500.0 - (i * 150.0), built.CentreY);
            if (block >= 0)
            {
                blocks.Add(block);
            }
        }

        if (blocks.Count < 2)
        {
            notes.Add("no spare housing would stand near the centre");
        }

        for (var i = 0; i + 1 < blocks.Count; i++)
        {
            var id = world.Buildings.IdAt(blocks[i]);
            for (var head = 0; head < T.BResidents[world.Buildings.KindAt(blocks[i])]; head++)
            {
                world.Citizens.AddArrival(id, 20 + (head % 35));
            }
        }

        // And an ordinary site — placed, not contracted — so the republic's own
        // gangs have something to work and something to eat while working it.
        // Out at the far corner, because a gang that can walk to its site is a
        // gang no bus is ever sent for.
        if (Scenario.FindSite(world, T.BuildingIndex("House"), far.X, far.Y, 700.0) is { } plot)
        {
            world.Issue(Command.Place(T.BuildingIndex("House"), plot.X, plot.Y));
        }
        else
        {
            notes.Add("nowhere out there would take an ordinary house site");
        }

        // Hands from abroad, standing at a post and needing a lift.
        if (built.ConstructionOffice >= 0)
        {
            var office = world.Buildings.IdAt(built.ConstructionOffice);
            if (!world.Issue(Command.HireForeign(Market.East, office, 10)).Accepted)
            {
                notes.Add("the East holds no post this founding could hire through");
            }
        }

        // A way and a span under construction, so the construction pass has all
        // three kinds of site to work — and enough gravel on hand to finish
        // them, because a site with no materials is a site nothing is posted to.

        var ordered = world.Issue(Command.OrderRoad(
            built.CentreX, built.CentreY,
            built.CentreX + 900.0, built.CentreY + 300.0, dirt, false));
        if (!ordered.Accepted)
        {
            notes.Add($"no dirt track could be ordered: {ordered.Refusal}");
        }

        // A lit one as well, and finished on the spot: street lighting is only
        // something the power pass can see once the way is in the network.
        var lit = world.Issue(Command.OrderRoad(
            built.CentreX - 100.0, built.CentreY,
            built.CentreX - 700.0, built.CentreY + 200.0, paved, true));
        if (lit.Accepted)
        {
            Finish(world, lit.Id);
        }
        else
        {
            notes.Add($"no lit way could be ordered: {lit.Refusal}");
        }

        if (built.Plant >= 0 && built.Farm >= 0)
        {
            var span = world.Issue(Command.OrderLine(
                T.UtilityIndex("Power"),
                world.Buildings.XAt(built.Plant), world.Buildings.YAt(built.Plant),
                world.Buildings.XAt(built.Farm), world.Buildings.YAt(built.Farm)));
            if (!span.Accepted)
            {
                notes.Add($"no span could be ordered to the farm: {span.Refusal}");
            }
        }

        // A belt from the pit to the power station, energised: what a conveyor
        // buys is a haul with no lorry and no diesel, and nothing else in the
        // republic exercises that path.
        if (built.Mine >= 0 && built.Plant >= 0
            && !Belt(world, "Conveyor", built.Mine, built.Plant))
        {
            notes.Add("no belt could be run from the pit to the plant");
        }

        // Something for a foreign firm to build, so the contracting bill runs.
        if (Scenario.FindSite(
                world, T.BuildingIndex("Warehouse"),
                built.CentreX + 500.0, built.CentreY - 500.0, 900.0) is { } yard)
        {
            world.Issue(Command.ContractBuild(
                T.BuildingIndex("Warehouse"), yard.X, yard.Y, Market.West));
        }
        else
        {
            notes.Add("nowhere would take a contracted warehouse");
        }

        // Standing instructions in both directions, and goods at the post for
        // the outward one to reach.
        world.Issue(Command.AddTradeRule(T.ResourceIndex("Coal"), Market.East, TradeAction.Sell));
        world.Issue(Command.AddTradeRule(T.ResourceIndex("Bricks"), Market.East, TradeAction.Buy));
        if (built.Customs >= 0)
        {
            world.Buildings.Stock.Add(built.Customs, T.ResourceIndex("Coal"), 150.0);
        }
        else
        {
            notes.Add("this founding reached no frontier post");
        }

        // Children, so the schools have somebody to teach. A founding of
        // settlers is all adults and the first native cohort is six years off,
        // which is longer than any test should run.
        foreach (var block in built.Housing)
        {
            for (var i = 0; i < 4; i++)
            {
                world.Citizens.Add(world.Buildings.IdAt(block), 8 + i, 0, 1.0, 0.5);
            }
        }

        // And an instruction about where sites buy what the republic cannot
        // make. The customs house the founding opened is on the nearest post,
        // so that is the post the policy names — goods land there and the
        // lorries still have to fetch them.
        var through = world.Frontier.NearestCrossing(built.CentreX, built.CentreY, null);
        if (through is not null)
        {
            world.Issue(Command.SetImportPolicy(null, through.Value.Id));
        }

        // An advance that will not be repaid, so the loans pass has a default
        // to settle rather than a term nobody reaches.
        if (!world.Issue(Command.TakeLoan(Market.West, 0)).Accepted)
        {
            notes.Add("the West would not advance anything");
        }

        report = notes.Count == 0
            ? "the fixture stood up in full"
            : "the fixture fell short: " + string.Join("; ", notes);

        return world;
    }

    private static int Grade(string id) => T.GradeIndex(id);

    /// <summary>
    /// The furthest corner of the map a way will actually reach, and open the
    /// way to it.
    /// </summary>
    /// <remarks>
    /// Further than anybody will walk, so getting to work out there is a seat
    /// on something rather than a stroll — which is the only way the labour
    /// pass ever burns a drop of diesel. The road is opened on the spot, as the
    /// founding grant's own lines are: without one there is no bus route and
    /// nobody can reach the place at all.
    /// </remarks>
    private static (double X, double Y) Outpost(
        World world, double fromX, double fromY, int dirt, out bool reached)
    {
        var edge = world.Terrain.Extent - 250.0;
        var middle = world.Terrain.Extent / 2.0;
        var targets = new List<(double X, double Y)>
        {
            (250.0, 250.0), (edge, 250.0), (250.0, edge), (edge, edge),
            (middle, 250.0), (middle, edge), (250.0, middle), (edge, middle),
        };

        // Furthest first, and only somewhere genuinely past walking range:
        // a works anybody can stroll to is a works no bus is ever run for.
        targets.RemoveAll(c => Units.Distance(c.X, c.Y, fromX, fromY) <= T.MaxWalkM * 1.2);
        targets.Sort((a, b) => Units.Distance(b.X, b.Y, fromX, fromY)
            .CompareTo(Units.Distance(a.X, a.Y, fromX, fromY)));

        var bridge = Grade("Bridge");
        foreach (var target in targets)
        {
            // A dirt track if the ground allows, and a bridge where it does
            // not: the refusal names open water, and open water is what a
            // bridge is for.
            foreach (var grade in new[] { dirt, bridge })
            {
                var run = world.Issue(Command.OrderRoad(
                    fromX, fromY, target.X, target.Y, grade, false));
                if (run.Accepted)
                {
                    Finish(world, run.Id);
                    reached = true;
                    return target;
                }
            }
        }

        reached = false;
        return targets.Count > 0 ? targets[0] : (250.0, 250.0);
    }

    /// <summary>Open an ordered way there and then, as the founding grant does.</summary>
    private static void Finish(World world, int site)
    {
        var run = world.RoadWorks.Get(site);
        if (run is null)
        {
            return;
        }

        run.WorkDone = run.Labour(T);
        world.RoadWorks.Finish(run);
        world.Reopen(run);
    }

    /// <summary>Run a belt between two buildings and energise it.</summary>
    private static bool Belt(World world, string utility, int from, int to)
    {
        var kind = T.UtilityIndex(utility);
        var refusal = world.LineWorks.Order(
            kind,
            world.Buildings.XAt(from), world.Buildings.YAt(from),
            world.Buildings.XAt(to), world.Buildings.YAt(to),
            world.Buildings.Commissioned, out var site);

        if (refusal != LineError.None || site is null)
        {
            return false;
        }

        world.LineWorks.Finish(site);
        world.Grid.Energise(site);
        world.Grid.AttachAlong(world.Buildings, kind);
        return true;
    }

    /// <summary>
    /// The pokes that keep it in every corner — the things a player would be
    /// doing that a scripted opening cannot decide for itself.
    /// </summary>
    public static void Nudge(World world, int day)
    {
        ArgumentNullException.ThrowIfNull(world);

        // A tender every fortnight, accepted and never delivered against, so
        // the contracts sweep has something to fail and fine.
        if (day % 14 == 0)
        {
            world.Contracts.Offer(
                Market.East, T.ResourceIndex("Coal"), world.Clock.DayIndex, world.Rng);

            foreach (var tender in world.Contracts.Offers())
            {
                world.Issue(Command.AcceptContract(tender.Id));
            }
        }

        // Settlers and visitors at the frontier. Put there rather than waited
        // for: whether the republic is attractive enough to draw any is balance,
        // and this fixture is about reach rather than about balance.
        if (day % 30 == 7 && world.Frontier.Crossings.Count > 0)
        {
            var post = world.Frontier.Crossings[0];
            world.Migration.Arrive(post.X, post.Y, 6, world.Clock.DayIndex);
            world.Tourism.Arrive(post.X, post.Y, 4, post.Bloc, world.Clock.DayIndex, T);
        }
    }

    /// <summary>Stand a kind up finished near a point, or -1.</summary>
    private static int Stand(World world, string id, double x, double y)
    {
        var kind = T.BuildingIndex(id);
        var at = Scenario.FindSite(world, kind, x, y, 700.0);
        if (at is null)
        {
            return -1;
        }

        var placed = world.Issue(Command.Place(kind, at.Value.X, at.Value.Y));
        if (!placed.Accepted)
        {
            return -1;
        }

        var i = world.Buildings.IndexOf(placed.Id);
        world.Buildings.AddWork(i, T.BLabour[kind]);
        world.Grid.AttachAll(placed.Id, at.Value.X, at.Value.Y);
        return i;
    }
}
