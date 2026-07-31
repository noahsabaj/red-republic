namespace RedRepublic.Sim;

/// <summary>
/// One passenger service the republic runs today: the way it rides, and how many
/// seats are left on it.
/// </summary>
/// <remarks>
/// Separate pools per way, deliberately. One pool would mean a republic whose
/// trams are full could not put anybody on a bus, and would make the choice
/// between laying tramway and buying more buses mean nothing.
/// </remarks>
public record struct Service(Medium Medium, int Seats);

/// <summary>
/// Getting to work: the journey, and what extends how far one can reach.
/// </summary>
/// <remarks>
/// <para>
/// A job you cannot reach is not a job, and until this existed the only way to
/// reach one was to walk. That is a complete model of a village and a useless
/// model of a city: it means a republic can never be larger than the distance a
/// person will walk, and that every remote deposit needs its own town whether or
/// not the geography wants one there.
/// </para>
/// <para>
/// Transport lifts that ceiling <b>physically</b>. A bus rides the road network,
/// so it reaches exactly what the republic has built road to and nothing else. It
/// carries a finite number of people, so extending reach is a capacity you fund
/// rather than a rule that changes. And it burns fuel, so a commute is an ongoing
/// cost against the same refinery output everything else wants.
/// </para>
/// <para>
/// The speeds are <b>commercial</b> speeds — stopping, boarding and waiting
/// already included, which is what a transit authority quotes because it is what
/// a passenger experiences. Modelling a bus at road speed and subtracting dwell
/// time separately would arrive at the same place with more machinery.
/// </para>
/// </remarks>
public static class Transport
{
    /// <summary>
    /// How a citizen could get from home to work, over every way the republic
    /// actually runs a service on, or <c>null</c> if they could not.
    /// </summary>
    /// <remarks>
    /// <para>
    /// <b>Walking always wins when it is possible</b>, because a seat spent on
    /// somebody who could have walked is a seat denied to somebody who could not.
    /// Beyond that the quickest way with a seat left wins. Any cleverer scheme
    /// would be the simulation deciding who deserves the metro, which is not its
    /// call.
    /// </para>
    /// <para>
    /// <paramref name="inTheDark"/> is what makes street lighting a mechanic
    /// rather than a decoration. A ride is a ride whatever the hour — the
    /// passenger is not the one out in the dark — so a republic can answer the
    /// night with lamps <i>or</i> with a service, and both cost. A short walk is
    /// always fine. Beyond that the whole route must be lit, and the search only
    /// ever traverses lit road, so a longer lit way round beats a shorter dark
    /// one: exactly the position a republic that has just lit its main street is
    /// in.
    /// </para>
    /// </remarks>
    public static Commute? ReachAt(
        World world, double homeX, double homeY, double workX, double workY,
        IReadOnlyList<Service> services, bool inTheDark)
    {
        ArgumentNullException.ThrowIfNull(world);
        ArgumentNullException.ThrowIfNull(services);
        var t = world.Tables;

        if (inTheDark)
        {
            var across = Units.Distance(homeX, homeY, workX, workY);
            if (across <= t.NightWalkM)
            {
                return Commute.OnFoot(across, t);
            }

            var lit = LitWalk(world, homeX, homeY, workX, workY);
            if (lit is not null)
            {
                return lit;
            }
        }
        else
        {
            var walk = CommuteDistance(world.Roads, homeX, homeY, workX, workY, t);
            if (walk <= t.MaxWalkM)
            {
                return Commute.OnFoot(walk, t);
            }
        }

        foreach (var service in Ranked(services, t))
        {
            var ride = RideOn(
                world.NetworkFor(service.Medium), homeX, homeY, workX, workY, service.Medium, t);
            if (ride is not null)
            {
                return ride;
            }
        }

        return null;
    }

    /// <summary>
    /// The walking distance from a home to a workplace.
    /// </summary>
    /// <remarks>
    /// Over the road network when both ends are near one — walking round a lake
    /// on a road is a real route where the straight line through it is not — and
    /// the straight line otherwise, which is the right answer on open ground. It
    /// never claims a road is longer than walking directly.
    /// </remarks>
    public static double CommuteDistance(
        Network roads, double homeX, double homeY, double workX, double workY, Tables t)
    {
        ArgumentNullException.ThrowIfNull(roads);
        ArgumentNullException.ThrowIfNull(t);

        var straight = Units.Distance(homeX, homeY, workX, workY);
        var a = roads.NearestNode(homeX, homeY, t.RoadAccessM);
        var b = roads.NearestNode(workX, workY, t.RoadAccessM);
        if (a < 0 || b < 0)
        {
            return straight;
        }

        var route = roads.RouteBetween(a, b);
        if (route is null)
        {
            return straight;
        }

        var byRoad = Units.Distance(roads.NodeX(a), roads.NodeY(a), homeX, homeY)
            + route.Distance
            + Units.Distance(roads.NodeX(b), roads.NodeY(b), workX, workY);

        return byRoad < straight ? byRoad : straight;
    }

    /// <summary>Whether somebody living at one point could walk to a job at another.</summary>
    /// <remarks>
    /// On foot only. Whether they could get there at all is
    /// <see cref="ReachAt"/>, which also knows about seats.
    /// </remarks>
    public static bool IsReachable(
        Network roads, double homeX, double homeY, double workX, double workY, Tables t)
    {
        ArgumentNullException.ThrowIfNull(t);
        return CommuteDistance(roads, homeX, homeY, workX, workY, t) <= t.MaxWalkM;
    }

    /// <summary>
    /// Every passenger service the republic runs today, one entry per way.
    /// </summary>
    /// <remarks>
    /// <b>Read off authored fields, never off a list of building kinds.</b> A
    /// building runs a service when it has seats, and its medium says over what —
    /// so a fifth passenger mode is a data row and nothing here changes. A list
    /// of kinds inside a system is a thing somebody has to remember to edit, and
    /// what they forget lands silently in a fallback.
    /// </remarks>
    public static List<Service> Services(World world)
    {
        ArgumentNullException.ThrowIfNull(world);
        var t = world.Tables;
        var found = new List<Service>();

        foreach (var medium in Enum.GetValues<Medium>())
        {
            var seats = 0;
            for (var b = 0; b < world.Buildings.Count; b++)
            {
                var kind = world.Buildings.KindAt(b);
                if (!world.Buildings.IsBuilt(b) || t.BSeats[kind] <= 0
                    || t.BMedium[kind] != (int)medium)
                {
                    continue;
                }

                seats += (int)(t.BSeats[kind] * RunningShare(world, b));
            }

            if (seats > 0)
            {
                found.Add(new Service(medium, seats));
            }
        }

        return found;
    }

    /// <summary>
    /// Book seats against the depots that provided them, and report the fuel each
    /// one burns.
    /// </summary>
    /// <remarks>
    /// Only the depots that burn something: an electric service draws its power
    /// through the grid like any other building and has no fuel bill. Drawn down
    /// in id order so the answer is reproducible, and each filled to its own
    /// running capacity before the next is touched — a half-used fleet is one
    /// depot working and another idle, not two depots half-idling.
    /// </remarks>
    public static List<(int Depot, double Tonnes)> FuelBurn(World world, int used)
    {
        ArgumentNullException.ThrowIfNull(world);
        var t = world.Tables;
        var fuel = t.ResourceIndex("Fuel");
        var burned = new List<(int, double)>();

        for (var b = 0; b < world.Buildings.Count && used > 0; b++)
        {
            var kind = world.Buildings.KindAt(b);
            if (!world.Buildings.IsBuilt(b) || t.BSeats[kind] <= 0 || !Burns(t, kind, fuel))
            {
                continue;
            }

            var capacity = (int)(t.BSeats[kind] * RunningShare(world, b));
            var taken = Math.Min(used, capacity);
            if (taken <= 0)
            {
                continue;
            }

            used -= taken;
            burned.Add((b, taken * t.FuelPerSeatDay));
        }

        return burned;
    }

    /// <summary>Whether a building is close enough to a road to be served by freight.</summary>
    public static bool IsRoadServed(Network roads, double x, double y, Tables t)
    {
        ArgumentNullException.ThrowIfNull(roads);
        ArgumentNullException.ThrowIfNull(t);
        return roads.NearestNode(x, y, t.RoadAccessM) >= 0;
    }

    /// <summary>The carried half: a ride over one way, or <c>null</c>.</summary>
    /// <remarks>
    /// <c>null</c> when either end is out of walking range of the network, when
    /// the two are not connected, or when the whole journey would take longer
    /// than anybody will travel — a bound on <b>time</b> rather than distance,
    /// which is the point of having transport at all: the same three quarters of
    /// an hour is a short walk or a long ride, and that difference is the
    /// mechanic.
    /// </remarks>
    private static Commute? RideOn(
        Network net, double homeX, double homeY, double workX, double workY, Medium medium, Tables t)
    {
        var start = net.NearestNode(homeX, homeY, t.StopWalkM);
        var finish = net.NearestNode(workX, workY, t.StopWalkM);
        if (start < 0 || finish < 0)
        {
            return null;
        }

        var route = net.RouteBetween(start, finish);
        if (route is null)
        {
            return null;
        }

        var walk = Units.KphToMps(t.WalkKph);
        var toStop = Units.Distance(net.NodeX(start), net.NodeY(start), homeX, homeY);
        var fromStop = Units.Distance(net.NodeX(finish), net.NodeY(finish), workX, workY);
        var time = Units.TimeToCover(walk, toStop)
            + Units.TimeToCover(Units.KphToMps(t.CommercialKph((int)medium)), route.Distance)
            + Units.TimeToCover(walk, fromStop);

        return time > t.MaxCommuteS
            ? null
            : Commute.Carried((int)medium, toStop + route.Distance + fromStop, time);
    }

    /// <summary>
    /// A walk to work along lit road only, or <c>null</c> if there is not one
    /// inside the ordinary walking bound.
    /// </summary>
    /// <remarks>
    /// Both ends still have to be within road access of a junction, and the last
    /// few metres off the road are counted the way the daytime route counts them:
    /// somebody walking from a lit street to a door thirty metres away is not
    /// lost in the dark.
    /// </remarks>
    private static Commute? LitWalk(
        World world, double homeX, double homeY, double workX, double workY)
    {
        var t = world.Tables;
        var roads = world.Roads;
        var a = roads.NearestNode(homeX, homeY, t.RoadAccessM);
        var b = roads.NearestNode(workX, workY, t.RoadAccessM);
        if (a < 0 || b < 0)
        {
            return null;
        }

        var route = roads.RouteWhere(a, b, static s => s.IsLit);
        if (route is null)
        {
            return null;
        }

        var distance = Units.Distance(roads.NodeX(a), roads.NodeY(a), homeX, homeY)
            + route.Distance
            + Units.Distance(roads.NodeX(b), roads.NodeY(b), workX, workY);

        return distance <= t.MaxWalkM ? Commute.OnFoot(distance, t) : null;
    }

    /// <summary>Services with seats left, quickest way first, ties by medium.</summary>
    private static List<Service> Ranked(IReadOnlyList<Service> services, Tables t)
    {
        var ranked = new List<Service>();
        foreach (var s in services)
        {
            if (s.Seats > 0)
            {
                ranked.Add(s);
            }
        }

        ranked.Sort((a, b) =>
        {
            var bySpeed = t.CommercialKph((int)b.Medium).CompareTo(t.CommercialKph((int)a.Medium));
            return bySpeed != 0 ? bySpeed : a.Medium.CompareTo(b.Medium);
        });

        return ranked;
    }

    /// <summary>
    /// What share of a depot's vehicles can roll: its staffing, and whatever it
    /// runs on.
    /// </summary>
    /// <remarks>
    /// <b>A depot runs on one of two things and says which.</b> One with fuel
    /// among its inputs is limited by its tank; one without is electric and is
    /// limited by whether the grid is reaching it. Authored rather than matched
    /// on by kind — and it is the whole trolleybus trade, because a republic that
    /// strings wire runs its buses on its own generation instead of on oil it may
    /// have to buy.
    /// </remarks>
    private static double RunningShare(World world, int b)
    {
        var t = world.Tables;
        var kind = world.Buildings.KindAt(b);
        var staffing = world.Buildings.Staffing(b);
        var fuel = t.ResourceIndex("Fuel");

        if (!Burns(t, kind, fuel))
        {
            return world.Buildings.PoweredAt(b) ? staffing : 0.0;
        }

        // A depot with no fuel runs no buses at all and one with a little runs a
        // few — the same shape as every other input limiter, and for the same
        // reason: a shortage should degrade a service rather than switch it off
        // at a threshold nobody can see.
        var wanted = t.BSeats[kind] * t.FuelPerSeatDay;
        var held = world.Buildings.Stock.Get(b, fuel);
        var share = wanted <= 0.0 ? 1.0 : Math.Clamp(held / wanted, 0.0, 1.0);
        return Math.Min(staffing, share);
    }

    private static bool Burns(Tables t, int kind, int resource)
    {
        foreach (var r in t.Inputs.KeysOf(kind))
        {
            if (r == resource)
            {
                return true;
            }
        }

        return false;
    }
}
