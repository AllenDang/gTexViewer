# gTexViewer

A fast and intuitive texture viewer designed for game developers and digital artists. Easily preview textures from games, 3D models, and various image formats with advanced viewing features.

## What is gTexViewer?

gTexViewer is a specialized image viewer that understands game textures and 3D model formats. Whether you're working with standard images, compressed game textures, or embedded textures from 3D models, gTexViewer provides the tools you need to inspect and analyze your visual assets.

## Key Features

### 🎮 Game Format Support
- **KTX2 textures** - View compressed game textures with Basis Universal transcoding
- **Compressed textures**: DDS (BC1-BC7), ETC1/ETC2, EAC, PVRTC, ATC, ASTC
- **GLB/GLTF models** - Extract and preview embedded textures from 3D models
- **FBX files** - Access textures embedded in FBX models
- **ZIP archives** - Browse and view textures inside compressed archives

### 🖼️ Standard Image Formats
- **Common formats**: PNG, JPEG, BMP, GIF, TIFF, WebP, TGA
- **Advanced formats**: AVIF/HEIF, HDR, EXR, QOI, Farbfeld
- **Legacy formats**: ICO, PNM (PGM, PPM, PAM)

### 🔍 Advanced Viewing Tools
- **Channel Switching** - View individual RGBA channels (Red, Green, Blue, Alpha) to inspect texture data
- **Pixel-Perfect Zoom** - Examine textures at 1:1 pixel ratio for detailed inspection
- **Smooth Scaling** - Seamless zooming from 0.01x to 10x+ magnification
- **Pan & Zoom** - Navigate large textures with smooth camera controls

### 📋 Multi-Image Viewing
- **Drag & Drop** - Load multiple images at once by dropping them into the window
- **Smart Layout** - Automatically arranges multiple images for optimal viewing
- **Batch Processing** - Compare textures side-by-side with adaptive sizing

### ℹ️ Texture Information
- **Hover Tooltips** - Get instant texture information (format, dimensions, file size)
- **Format Details** - See color space and compression information
- **Loading Progress** - Visual indicators show loading status for large files

## How to Use

### Getting Started
1. Launch gTexViewer
2. Drag and drop your texture files into the window
3. Use mouse wheel to zoom, click and drag to pan
4. Hover over images to see detailed information

### Viewing Channels
- Press `R` to view red channel only
- Press `G` to view green channel only  
- Press `B` to view blue channel only
- Press `A` to view alpha channel only
- Press `Esc` to return to normal view

### Command Line Usage
```bash
# Open specific file
gtexviewer texture.png

# Open multiple files
gtexviewer texture1.png texture2.ktx2 model.glb
```

## Who Should Use gTexViewer?

- **Game Developers** - Preview and validate game textures during development
- **3D Artists** - Inspect textures embedded in models and check format compatibility
- **Technical Artists** - Analyze texture channels and compression artifacts
- **Asset Pipeline Engineers** - Verify texture processing and format conversion

## License

MIT License

## Author

**Allen Dang** - [allengnr@gmail.com](mailto:allengnr@gmail.com)