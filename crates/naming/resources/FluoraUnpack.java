// Saves every series of a CZI (or any Bio-Formats file) as its own ImageJ TIFF:
// <name>_series0.tif, <name>_series1.tif, ... the same as the lab's Fiji "Unpack" macro.
import ij.IJ;
import ij.ImagePlus;
import loci.formats.ImageReader;
import loci.formats.MetadataTools;
import loci.formats.meta.IMetadata;
import loci.plugins.BF;
import loci.plugins.in.ImporterOptions;
import java.io.File;

public class FluoraUnpack {
    public static void main(String[] a) throws Exception {
        String in = a[0], outDir = a[1];
        String base = new File(in).getName().replaceAll("\\.[^.]+$", "");
        ImageReader r = new ImageReader();
        IMetadata meta = MetadataTools.createOMEXMLMetadata();
        r.setMetadataStore(meta);
        r.setId(in);
        int n = r.getSeriesCount();
        r.close();
        System.out.println("FLUORA series " + n);
        for (int s = 0; s < n; s++) {
            File out = new File(outDir, base + "_series" + s + ".tif");
            ImporterOptions o = new ImporterOptions();
            o.setId(in);
            o.setAutoscale(true);
            o.setColorMode(ImporterOptions.COLOR_MODE_DEFAULT);
            o.setStackOrder(ImporterOptions.ORDER_XYCZT);
            o.setSeriesOn(s, true);
            o.setQuiet(true);
            ImagePlus[] imps = BF.openImagePlus(o);
            // Keep the channel (filter) names, in the form Zeiss uses, so Fluora and Fiji can show them
            StringBuilder info = new StringBuilder();
            int nc = meta.getChannelCount(s);
            for (int c = 0; c < nc; c++) {
                String name = meta.getChannelName(s, c);
                if (name != null) info.append("Information|Image|Channel|Name #").append(c + 1).append(" = ").append(name).append("\n");
            }
            // and where it came from, so Fluora can read the date and objective from the CZI
            info.append("Information|Fluora|SourceCzi = ").append(new File(in).getAbsolutePath()).append("\n");
            Object old = imps[0].getProperty("Info");
            if (old != null) info.append(old);
            if (info.length() > 0) imps[0].setProperty("Info", info.toString());
            File part = new File(outDir, base + "_series" + s + ".fluora-part.tif");
            IJ.saveAsTiff(imps[0], part.getPath());
            imps[0].close();
            if (!part.renameTo(out)) throw new Exception("Could not rename " + part);
            System.out.println("FLUORA saved " + s);
        }
        System.out.println("FLUORA done");
        System.exit(0);
    }
}
