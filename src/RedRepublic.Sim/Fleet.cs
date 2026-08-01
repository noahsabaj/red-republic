namespace RedRepublic.Sim;

/// <summary>
/// What a vehicle is doing.
/// </summary>
/// <remarks>
/// Deliberately finer than idle-or-running: the fleet system decides what
/// happens when a journey ends by looking at this, and "arrived somewhere" has
/// three different meanings depending on which way the lorry was pointed.
/// </remarks>
public enum VehicleState
{
    /// <summary>Parked at its garage, available for work.</summary>
    Idle,

    /// <summary>Driving empty to the supplier, or out to a casualty.</summary>
    Fetching,

    /// <summary>Driving laden to the destination.</summary>
    Delivering,

    /// <summary>Driving back to its garage.</summary>
    Returning,

    /// <summary>
    /// Stuck. It keeps its job and its plan and is going nowhere until the
    /// ground firms up under it or something comes and pulls it out.
    /// </summary>
    Bogged,
}

/// <summary>What kind of work a vehicle has been sent out to do.</summary>
public enum JobKind
{
    None,

    /// <summary>Take this much of this, from here to there.</summary>
    Haul,

    /// <summary>
    /// Go and pull that one out. A kind of its own rather than a haul with odd
    /// fields, because the journey is to a <i>vehicle</i> rather than to a
    /// place, and what happens on arrival is nothing like unloading.
    /// </summary>
    Recover,

    /// <summary>
    /// Carry builders from the office out to a site. The bus sets off
    /// <b>laden</b>, which is why this is not a fetch-then-deliver: the crew is
    /// at the office and the office is the bus's home, so there is nothing to
    /// drive out empty for.
    /// </summary>
    Ferry,

    /// <summary>Go and bring a crew back.</summary>
    Collect,

    /// <summary>Fetch settlers from a frontier post and take them to housing.</summary>
    Settle,

    /// <summary>Fetch visitors from a frontier post and take them to a hotel.</summary>
    Tour,

    /// <summary>
    /// Drive out there and back, pushing the snow off everything on the way.
    /// Named by a <i>place</i> rather than by a building, which is what makes it
    /// unlike every other job: a stretch of buried ground is not a consignee.
    /// There is nothing to load, nothing to set down and nobody to collect — the
    /// work is the journey.
    /// </summary>
    Plough,
}

/// <summary>What sort of thing a delivery is addressed to.</summary>
/// <remarks>
/// Freight used to reach only buildings. A road under construction is a site
/// with a bill of materials like any other and the gravel has to be
/// <i>driven</i> to it, so the delivery half of the simulation has to be able to
/// name one. Goods are never collected <i>from</i> a site: a site is a place
/// work goes into, not a yard.
/// </remarks>
public enum DestinationKind
{
    Building,
    RoadSite,
    LineSite,

    /// <summary>A bare place — what a plough is sent to.</summary>
    Place,
}

/// <summary>Somewhere goods, people or work can be sent.</summary>
public readonly record struct Destination(DestinationKind Kind, int Id, double X, double Y)
{
    public static Destination Building(int id) => new(DestinationKind.Building, id, 0.0, 0.0);

    public static Destination RoadSite(int id) => new(DestinationKind.RoadSite, id, 0.0, 0.0);

    public static Destination LineSite(int id) => new(DestinationKind.LineSite, id, 0.0, 0.0);

    public static Destination Place(double x, double y) => new(DestinationKind.Place, -1, x, y);
}

/// <summary>What a vehicle has been sent out to do.</summary>
public readonly record struct Job(
    JobKind Kind,
    int From,
    Destination To,
    int Resource,
    double Tonnes,
    int Subject)
{
    public static Job None { get; } = new(JobKind.None, -1, default, -1, 0.0, -1);

    public static Job Haul(int from, Destination to, int resource, double tonnes) =>
        new(JobKind.Haul, from, to, resource, tonnes, -1);

    public static Job Recover(int casualty) =>
        new(JobKind.Recover, -1, default, -1, 0.0, casualty);

    /// <summary>Go and get that gang, and take them out to that site.</summary>
    public static Job Ferry(Destination to, int party) =>
        new(JobKind.Ferry, -1, to, -1, 0.0, party);

    /// <summary>Go and get that gang, and bring them back to the office.</summary>
    public static Job Collect(int party, int office) =>
        new(JobKind.Collect, -1, Destination.Building(office), -1, 0.0, party);

    public static Job Settle(int group, int to) =>
        new(JobKind.Settle, -1, Destination.Building(to), -1, 0.0, group);

    public static Job Tour(int visit, int to) =>
        new(JobKind.Tour, -1, Destination.Building(to), -1, 0.0, visit);

    public static Job Plough(double x, double y) =>
        new(JobKind.Plough, -1, Destination.Place(x, y), -1, 0.0, -1);
}

/// <summary>
/// Every vehicle the republic owns.
/// </summary>
/// <remarks>
/// A vehicle belongs to a garage — its <i>establishment</i>, authored on the
/// building that keeps it. A depot with three shifts of drivers still has only
/// as many lorries as it owns, which is why staffing is a fill fraction and the
/// fleet is a count.
/// </remarks>
public sealed class Fleet(Tables tables)
{
    private readonly Tables _t = tables;
    private readonly List<int> _id = [];
    private readonly List<int> _kind = [];
    private readonly List<int> _home = [];
    private readonly List<VehicleState> _state = [];

    /// <summary>What it was doing when it stuck, so it can carry on doing it.</summary>
    private readonly List<VehicleState> _wasDoing = [];

    /// <summary>The day it stuck — how long it has been there decides whether a tow is sent.</summary>
    private readonly List<long> _boggedSince = [];
    private readonly List<double> _x = [];
    private readonly List<double> _y = [];
    private readonly List<Journey?> _journey = [];
    private readonly List<double> _fuel = [];
    private readonly List<Job> _job = [];

    private int _nextId = 1;

    public int Count => _id.Count;

    /// <summary>What each vehicle is carrying.</summary>
    public GrowableStock Cargo { get; } = new(tables.Resources.Length);

    public int IdAt(int i) => _id[i];

    public int KindAt(int i) => _kind[i];

    /// <summary>The building id of the garage it belongs to.</summary>
    public int HomeAt(int i) => _home[i];

    public VehicleState StateAt(int i) => _state[i];

    public double XAt(int i) => _x[i];

    public double YAt(int i) => _y[i];

    public Journey? JourneyAt(int i) => _journey[i];

    public double FuelAt(int i) => _fuel[i];

    public Job JobAt(int i) => _job[i];

    public long BoggedSinceAt(int i) => _boggedSince[i];

    public bool IsBogged(int i) => _state[i] == VehicleState.Bogged;

    /// <summary>
    /// What it is in the middle of — the same answer whether or not it is
    /// currently stuck in it.
    /// </summary>
    public VehicleState DoingAt(int i) => _state[i] == VehicleState.Bogged ? _wasDoing[i] : _state[i];

    public int Commission(int kind, int home, double x, double y)
    {
        _id.Add(_nextId++);
        _kind.Add(kind);
        _home.Add(home);
        _state.Add(VehicleState.Idle);
        _wasDoing.Add(VehicleState.Idle);
        _boggedSince.Add(0);
        _x.Add(x);
        _y.Add(y);
        _journey.Add(null);
        _fuel.Add(_t.VTank[kind]);
        _job.Add(Job.None);
        Cargo.Grow();
        return _id.Count - 1;
    }

    /// <summary>
    /// Put a vehicle back exactly as it was, keeping its id — what a load does.
    /// </summary>
    public int Restore(
        int id, int kind, int home, VehicleState state, VehicleState wasDoing,
        long boggedSince, double x, double y, double fuel, Job job, Journey? journey)
    {
        _id.Add(id);
        _kind.Add(kind);
        _home.Add(home);
        _state.Add(state);
        _wasDoing.Add(wasDoing);
        _boggedSince.Add(boggedSince);
        _x.Add(x);
        _y.Add(y);
        _journey.Add(journey);
        _fuel.Add(fuel);
        _job.Add(job);
        Cargo.Grow();
        _nextId = Math.Max(_nextId, id + 1);
        return _id.Count - 1;
    }

    public void RemoveAt(int i)
    {
        _id.RemoveAt(i);
        _kind.RemoveAt(i);
        _home.RemoveAt(i);
        _state.RemoveAt(i);
        _wasDoing.RemoveAt(i);
        _boggedSince.RemoveAt(i);
        _x.RemoveAt(i);
        _y.RemoveAt(i);
        _journey.RemoveAt(i);
        _fuel.RemoveAt(i);
        _job.RemoveAt(i);
        Cargo.RemoveAt(i);
    }

    public int IndexOf(int id)
    {
        for (var i = 0; i < _id.Count; i++)
        {
            if (_id[i] == id)
            {
                return i;
            }
        }

        return -1;
    }

    public void SetPosition(int i, double x, double y)
    {
        _x[i] = x;
        _y[i] = y;
    }

    public void SetJourney(int i, Journey? journey) => _journey[i] = journey;

    public void SetJob(int i, Job job) => _job[i] = job;

    public void SetFuel(int i, double tonnes) =>
        _fuel[i] = Math.Clamp(tonnes, 0.0, _t.VTank[_kind[i]]);

    public void SetState(int i, VehicleState state)
    {
        if (state == VehicleState.Bogged)
        {
            throw new ArgumentException("use Bog to get stuck, so the day is recorded", nameof(state));
        }

        _state[i] = state;
    }

    /// <summary>
    /// It stuck. Remembers what it was doing so it can carry on doing it, and
    /// the day, because how long it has been there decides whether a tow comes.
    /// </summary>
    public void Bog(int i, long day)
    {
        if (_state[i] == VehicleState.Bogged)
        {
            return;
        }

        _wasDoing[i] = _state[i];
        _state[i] = VehicleState.Bogged;
        _boggedSince[i] = day;
    }

    /// <summary>Pulled out, or the ground firmed up. It carries on where it left off.</summary>
    public void Unbog(int i)
    {
        if (_state[i] == VehicleState.Bogged)
        {
            _state[i] = _wasDoing[i];
        }
    }

    public int Running()
    {
        var n = 0;
        for (var i = 0; i < Count; i++)
        {
            if (_state[i] != VehicleState.Idle && _state[i] != VehicleState.Bogged)
            {
                n++;
            }
        }

        return n;
    }

    public int Bogged()
    {
        var n = 0;
        for (var i = 0; i < Count; i++)
        {
            if (_state[i] == VehicleState.Bogged)
            {
                n++;
            }
        }

        return n;
    }

    /// <summary>Every vehicle kept by one garage.</summary>
    public List<int> OfGarage(int buildingId)
    {
        var found = new List<int>();
        for (var i = 0; i < Count; i++)
        {
            if (_home[i] == buildingId)
            {
                found.Add(i);
            }
        }

        return found;
    }

    public int CountOfKind(int kind)
    {
        var n = 0;
        for (var i = 0; i < Count; i++)
        {
            if (_kind[i] == kind)
            {
                n++;
            }
        }

        return n;
    }

    /// <summary>How much of everything the fleet is carrying right now.</summary>
    public double CargoAfloat()
    {
        var total = 0.0;
        for (var i = 0; i < Count; i++)
        {
            total += Cargo.Total(i);
        }

        return total;
    }

    /// <summary>
    /// How fast this vehicle goes on its current leg, given the going.
    /// </summary>
    /// <remarks>
    /// Three things hold it down and they are deliberately separate: what it is
    /// capable of, what the way under it allows, and how bad the ground is. A
    /// lorry on a dirt track makes the track's speed whatever it is capable of.
    /// </remarks>
    public double SpeedOfLeg(int i, double drag)
    {
        var journey = _journey[i];
        if (journey is null)
        {
            return 0.0;
        }

        var kind = _kind[i];
        return journey.LegSpeed(
            Units.KphToMps(_t.VOnRoadKph[kind]),
            Units.KphToMps(_t.VCrossCountryKph[kind]),
            drag);
    }
}
