#include "RustelAudioShared.h"
#include <iostream>
#include <thread>
#include <chrono>
#include <cmath>

int main(int argc,char** argv) {
   if(argc<2) return 1;
   RustelAudioShared memory;
   if(!memory.Open(argv[1])) return 2;
   if(argc>2) {
      std::cout<<"opened"<<std::endl;
      auto start=RustelAudioShared::NowMillis();
      while(memory.ProducerAlive() && RustelAudioShared::NowMillis()-start<8000)
         std::this_thread::sleep_for(std::chrono::milliseconds(5));
      if(memory.ProducerAlive()) return 3;
      memory.Close();return 0;
   }
   uint64_t start=RustelAudioShared::NowMillis(), frames=0;
   while(RustelAudioShared::NowMillis()-start<1000) {
      auto head=memory.Load64(24);
      if(head>=32) for(uint64_t frame=head-32;frame<head;++frame) {
         for(size_t slot=0;slot<3;++slot) for(size_t ch=0;ch<2;++ch) {
            auto value=memory.Load64(4096+((slot*16384+(frame&16383))*2+ch)*8);
            if(uint32_t(value>>32)!=uint32_t(frame)) return 4;
            uint32_t bits=uint32_t(value);float sample;std::memcpy(&sample,&bits,4);
            float expected=slot==0?(ch==0?0.125f:-0.25f):slot==1?(ch==0?0.25f:0.5f):0.0f;
            if(sample!=expected) return 5;
         }
         ++frames;
      }
      memory.Store64(64,RustelAudioShared::NowMillis());
      std::this_thread::sleep_for(std::chrono::milliseconds(5));
   }
   if(memory.Load32(8)!=48000 || frames<128) return 6;
   std::cout<<"PASS native reader: "<<frames<<" tagged stereo frames"<<std::endl;
}
