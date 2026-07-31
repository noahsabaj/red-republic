using Godot;

namespace RedRepublic.World;

/// <summary>
/// What the world is lit and coloured like.
/// </summary>
/// <remarks>
/// <para>
/// One place for every value the renderer needs and the simulation does not
/// know: sun angle, sky colours, fog, the ground tints the terrain shader reads.
/// Presentation only — nothing here may change how a republic turns out.
/// </para>
/// <para>
/// It is a class rather than a set of constants because a look is a thing that
/// gets swapped: the same posting under a different sky is a way to judge the
/// art, and a hard-coded set of literals scattered through the world builder is
/// not swappable at all.
/// </para>
/// </remarks>
public sealed class Look
{
    /// <summary>
    /// A warm mid-morning sun.
    /// </summary>
    /// <remarks>
    /// Around thirty degrees is where a fifteen-metre building lays down a
    /// shadow about twenty-six metres long, which is what tells you it is a
    /// building and not a painted rectangle. The near-overhead angle this
    /// replaced was chosen against a renderer that drew no shadows at all, so
    /// nothing was lost by putting the sun where shadows are shortest — and
    /// everything about how ground and buildings read depends on it now.
    /// </remarks>
    public float SunElevation { get; init; } = 30.0f;

    public float SunAzimuth { get; init; } = 138.0f;

    public Color SunColour { get; init; } = new(1.0f, 0.96f, 0.90f);

    public float SunEnergy { get; init; } = 1.6f;

    /// <summary>
    /// Ambient comes off the sky, so this is only the multiplier on it. The
    /// colour is kept because the compatibility renderer has no sky ambient and
    /// falls back to it.
    /// </summary>
    public Color AmbientColour { get; init; } = new(0.60f, 0.66f, 0.72f);

    public float AmbientEnergy { get; init; } = 0.35f;

    /// <summary>
    /// A real zenith is much deeper than a gradient sky wants. The pale pair a
    /// two-colour gradient needs to avoid banding is not what a scattering sky
    /// needs.
    /// </summary>
    public Color SkyTop { get; init; } = new(0.11f, 0.28f, 0.58f);

    public Color SkyHorizon { get; init; } = new(0.66f, 0.76f, 0.84f);

    public Color GroundHorizon { get; init; } = new(0.40f, 0.44f, 0.45f);

    public Color FogColour { get; init; } = new(0.74f, 0.80f, 0.86f);

    /// <summary>
    /// Per metre. About fourteen per cent at three kilometres — depth, with the
    /// distance still legible.
    /// </summary>
    /// <remarks>
    /// The figure this replaced gave three per cent at three kilometres, which is
    /// not depth but nothing, and is why the far side of the map read as hard as
    /// the near side. The first correction overshot to twenty-four per cent,
    /// which is a genuinely hazy day and drowned the far half of the map once the
    /// ground had detail worth seeing.
    /// </remarks>
    public float FogDensity { get; init; } = 0.00005f;

    public Color Water { get; init; } = new(0.30f, 0.44f, 0.52f);

    public float ContourStrength { get; init; } = 1.0f;

    /// <summary>
    /// Enough to break the sky up and cast some variety of light, not enough to
    /// make every screenshot overcast.
    /// </summary>
    public float CloudCover { get; init; } = 0.45f;

    /// <summary>The look the game opens with.</summary>
    public static Look Default { get; } = new();
}
